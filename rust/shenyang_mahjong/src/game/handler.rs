use super::*;

impl ShenyangMahjongGameHandler {
    pub(super) fn current_loop_state(
        &self,
        room_service: &RoomService,
        room_key: &str,
    ) -> Option<LoopStateHandle> {
        let state = self.loop_state(room_key)?;
        let state_common = Arc::clone(&state.lock().unwrap().base);
        let room_common = room_service.room_common_state(room_key)?;
        let is_running = !state_common.lock().unwrap().stop_requested();
        (is_running && Arc::ptr_eq(&state_common, &room_common)).then_some(state)
    }

    fn handle_play(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Ok(payload) = RoomService::parse_payload::<WsShenyangMahjongPlayRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(loop_state) = self.current_loop_state(room_service, &room_key) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        let configs = room_service.room_configs(&room_key).unwrap_or_default();
        let mut dispatch = Dispatch::default();

        {
            let mut state = loop_state.lock().unwrap();
            if state.phase != ShenyangMahjongPhase::Play {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            if state.is_away(position) {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }

            if state.claim_window.is_some() {
                let (
                    claim_tile,
                    from_position,
                    is_rob_gang,
                    eligible_positions,
                    already_responded,
                    claim_matches_source,
                ) = {
                    let claim_window = state.claim_window.as_ref().unwrap();
                    (
                        claim_window.tile,
                        claim_window.from_position,
                        matches!(claim_window.kind, ClaimWindowKind::RobGang),
                        claim_window.eligible_positions.clone(),
                        claim_window.responses.contains_key(&position),
                        claim_window_matches_source(&state, claim_window),
                    )
                };
                if position == from_position
                    || !eligible_positions.contains(&position)
                    || already_responded
                {
                    return room_service.error_response(
                        session_id,
                        Routes::PLAY as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                }
                let hand = state.hands.get(&position).cloned().unwrap_or_default();
                let invalid_claim_tile_count = has_impossible_known_tile_count(&state, claim_tile);
                let can_claim_meld = position_can_claim_meld(&state, position);
                let response = match payload.action {
                    ShenyangMahjongAction::PASS => ClaimResponse::Pass,
                    ShenyangMahjongAction::HU => {
                        if !claim_matches_source
                            || !can_claim_hu_with_configs(&state, position, claim_tile, &configs)
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Hu
                    }
                    ShenyangMahjongAction::PENG => {
                        if is_rob_gang
                            || !claim_matches_source
                            || invalid_claim_tile_count
                            || !can_claim_meld
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        if !can_peng(&hand, claim_tile) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Peng
                    }
                    ShenyangMahjongAction::GANG => {
                        if is_rob_gang
                            || !claim_matches_source
                            || invalid_claim_tile_count
                            || !can_claim_meld
                            || state.wall_count() == 0
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        if !can_gang(&hand, claim_tile) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Gang
                    }
                    ShenyangMahjongAction::CHI => {
                        if is_rob_gang
                            || !claim_matches_source
                            || invalid_claim_tile_count
                            || !can_claim_meld
                            || !position_can_chi(&state, position, &configs)
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        if position != state.next_position(from_position)
                            || !can_chi(&hand, claim_tile, &payload.tiles)
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Chi {
                            consume_tiles: payload.tiles.clone(),
                        }
                    }
                    _ => {
                        return room_service.error_response(
                            session_id,
                            Routes::PLAY as i32,
                            WsResponseCode::NO_PERMISSION,
                        );
                    }
                };

                let all_received = {
                    let claim_window = state.claim_window.as_mut().unwrap();
                    claim_window.responses.insert(position, response);
                    claim_window
                        .eligible_positions
                        .iter()
                        .all(|item| claim_window.responses.contains_key(item))
                };
                state.set_action_received(true);
                if all_received {
                    resolve_claim_window(
                        room_service,
                        &room_key,
                        &mut state,
                        &configs,
                        &mut dispatch,
                    );
                }
            } else {
                if state.current_position != position {
                    return room_service.error_response(
                        session_id,
                        Routes::PLAY as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                }

                match payload.action {
                    ShenyangMahjongAction::DISCARD => {
                        let tile = payload
                            .target_tile
                            .or_else(|| payload.tiles.first().copied())
                            .unwrap_or_default();
                        let Some(hand) = state.hands.get(&position).cloned() else {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        };
                        if !tiles_in_hand(&hand, &[tile]) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        if !perform_discard_with_ting(
                            room_service,
                            &room_key,
                            &mut state,
                            &configs,
                            &mut dispatch,
                            DiscardAction {
                                position,
                                tile,
                                declare_ting: payload.declare_ting.unwrap_or(false),
                            },
                        ) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                    }
                    ShenyangMahjongAction::HU => {
                        if !can_self_draw_hu_with_configs(&state, position, &configs) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        perform_self_draw_hu(
                            room_service,
                            &room_key,
                            &mut state,
                            &configs,
                            &mut dispatch,
                            position,
                        );
                    }
                    ShenyangMahjongAction::GANG => {
                        let tile = payload
                            .target_tile
                            .or_else(|| payload.tiles.first().copied())
                            .unwrap_or_default();
                        if !can_self_gang(&state, position, tile)
                            || !perform_self_gang(
                                room_service,
                                &room_key,
                                &mut state,
                                &configs,
                                &mut dispatch,
                                position,
                                tile,
                            )
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                    }
                    ShenyangMahjongAction::XI_GANG => {
                        if !perform_xi_gang(
                            room_service,
                            &room_key,
                            &mut state,
                            &configs,
                            &mut dispatch,
                            position,
                            &payload.tiles,
                        ) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                    }
                    _ => {
                        return room_service.error_response(
                            session_id,
                            Routes::PLAY as i32,
                            WsResponseCode::NO_PERMISSION,
                        );
                    }
                }
            }
        }

        room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
        dispatch
    }

    pub(super) fn handle_start(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
    ) -> Dispatch {
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        if position != 0 {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }

        let mut dispatch = Dispatch::default();
        if !room_service.require_room_membership(session_id, Routes::START as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        };
        if !room_service.room_is_ready_to_start(&room_key) {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        let Some(mut shared_common_state) = room_service.room_common_state(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        if shared_common_state.lock().unwrap().stop_requested() {
            let Some(next_common_state) =
                room_service.reset_room_common_state_for_new_game(&room_key)
            else {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            shared_common_state = next_common_state;
        }

        if let Some(existing) = self.loop_state(&room_key) {
            let same_state = {
                let state = existing.lock().unwrap();
                Arc::ptr_eq(&state.base, &shared_common_state)
            };
            if same_state {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            self.loop_states.lock().unwrap().remove(&room_key);
        }

        let loop_state = Arc::new(std::sync::Mutex::new(ShenyangMahjongLoopState::new(
            Arc::clone(&shared_common_state),
        )));
        room_service.set_room_game_state(
            &room_key,
            Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
                &loop_state,
            ))),
        );
        self.loop_states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&loop_state));

        crate::official::create_match(room_service, &room_key);
        {
            let state = loop_state.lock().unwrap();
            state.set_turn_countdown(0);
        }

        if let (Some(room_service_arc), Some(senders_arc)) =
            (self.room_service.as_ref(), self.senders.as_ref())
        {
            start_game_loop(
                room_key.clone(),
                loop_state,
                Arc::clone(room_service_arc),
                Arc::clone(senders_arc),
                Arc::clone(&self.loop_states),
            );
        }

        room_service.broadcast(
            &room_key,
            WsCode::START as i32,
            serde_json::json!({}),
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
        dispatch
    }

    pub(super) fn loop_state(&self, room_key: &str) -> Option<LoopStateHandle> {
        self.loop_states.lock().unwrap().get(room_key).cloned()
    }

    pub(super) fn prune_stopped_loop_states(&self, room_service: &mut RoomService) {
        let stopped = {
            let mut states = self.loop_states.lock().unwrap();
            let mut stopped = Vec::new();
            states.retain(|room_key, state| {
                let state = state.lock().unwrap();
                if state.stop_requested() {
                    stopped.push((room_key.clone(), Arc::clone(&state.base)));
                    false
                } else {
                    true
                }
            });
            stopped
        };
        for (room_key, common) in stopped {
            room_service.clear_room_game_state_if_same(&room_key, &common);
        }
    }
}

impl Default for ShenyangMahjongGameHandler {
    fn default() -> Self {
        Self {
            room_service: None,
            senders: None,
            loop_states: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl GameHandler for ShenyangMahjongGameHandler {
    fn supports_ai_players(&self) -> bool {
        cfg!(feature = "official")
    }

    #[cfg(feature = "official")]
    fn authorize_room_creation(
        &self,
        join: &share_type_public::WsJoinRequest,
    ) -> ws_common::MembershipAuthorization {
        Box::pin(crate::official::has_active_membership(
            join.session_id.clone(),
        ))
    }

    #[cfg(feature = "official")]
    fn authorize_ai_takeover(
        &self,
        official_session_id: String,
    ) -> ws_common::MembershipAuthorization {
        Box::pin(crate::official::has_active_membership(official_session_id))
    }

    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        if matches!(request.route, r if r == Routes::QUIT as i32 || r == Routes::DISBAND as i32) {
            self.prune_stopped_loop_states(room_service);
        }
        if request.route != Routes::JOIN as i32 || !join_succeeded(dispatch, session_id) {
            return;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return;
        };
        let Some(position) = room_service.session_position(session_id) else {
            return;
        };
        let Some(loop_state) = self.current_loop_state(room_service, &room_key) else {
            return;
        };
        let configs = room_service.room_configs(&room_key).unwrap_or_default();
        let state = loop_state.lock().unwrap();
        if state.phase == ShenyangMahjongPhase::Start {
            return;
        }
        push_direct_event(
            dispatch,
            session_id,
            WsCode::TABLE_SNAPSHOT as i32,
            build_table_snapshot_event_with_configs(&state, position, &configs),
        );
    }

    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_shenyang_mahjong_settings()
    }

    fn game_id(&self) -> GameId {
        GameId::SHENYANG_MAHJONG
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        match request.route {
            r if r == Routes::START as i32 => self.handle_start(room_service, session_id),
            r if r == Routes::PLAY as i32 => {
                self.handle_play(room_service, session_id, request.data)
            }
            _ => {
                room_service.error_response(session_id, request.route, WsResponseCode::NOT_IN_RANGE)
            }
        }
    }

    fn set_context(&mut self, senders: SessionSenders, room_service: Arc<Mutex<RoomService>>) {
        self.senders = Some(senders);
        self.room_service = Some(room_service);
    }
}
