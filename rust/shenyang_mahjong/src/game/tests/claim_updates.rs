use super::*;

pub(super) fn rob_gang_room() -> (RoomService, ShenyangMahjongGameHandler, LoopStateHandle) {
    let (mut room, mut handler, _, shared) = setup_request_room();
    {
        let mut state = shared.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state.hands = HashMap::from([
            (0, vec![1, 2, 3, 6, 11, 12, 13, 21, 22, 23, 31]),
            (1, vec![4, 5, 11, 12, 13, 21, 22, 23, 35, 35]),
            (2, vec![4, 5, 14, 15, 16, 24, 25, 26, 35, 35]),
            (3, vec![7, 8, 9, 17, 18, 19, 27, 28, 29, 33, 33, 34, 34]),
        ]);
        state.melds = HashMap::from([
            (0, vec![open_peng_meld(6, 2)]),
            (1, vec![open_peng_meld(31, 3)]),
            (2, vec![open_peng_meld(32, 3)]),
        ]);
        state.wall = vec![36, 37];
        // 加杠用手中的旧牌，当前摸到的牌与它不同。
        state.last_drawn_tile = Some(31);
    }
    let opened = handler.handle_game_request(
        &mut room,
        1,
        play_request(ShenyangMahjongAction::GANG, vec![6], Some(6), None),
    );
    assert_eq!(
        response_code(&opened, 1, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    let state = shared.lock().unwrap();
    let window = state
        .claim_window
        .as_ref()
        .expect("legal added gang opens a window");
    assert_eq!(window.eligible_positions, vec![1, 2]);
    assert!(matches!(window.kind, ClaimWindowKind::RobGang));
    drop(state);
    (room, handler, shared)
}

fn claim_for(dispatch: &Dispatch, recipient: SessionId) -> WsShenyangMahjongClaimWindowEvent {
    let events = dispatch
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            OutboundPayload::Event(event)
                if message.recipient == recipient && event.code == WsCode::CLAIM_WINDOW as i32 =>
            {
                Some(serde_json::from_value(event.data.clone()).expect("claim payload"))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1, "one private refresh per viewer");
    events.into_iter().next().unwrap()
}

pub(super) fn plays_for(
    dispatch: &Dispatch,
    recipient: SessionId,
) -> Vec<WsShenyangMahjongPlayEvent> {
    dispatch
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            OutboundPayload::Event(event)
                if message.recipient == recipient && event.code == WsCode::PLAY as i32 =>
            {
                Some(serde_json::from_value(event.data.clone()).expect("play payload"))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn partial_rob_gang_responses_refresh_private_options_without_resolving_or_resetting_time() {
    for first_position in [1, 2] {
        for action in [ShenyangMahjongAction::PASS, ShenyangMahjongAction::HU] {
            let (mut room, mut handler, shared) = rob_gang_room();
            // 模拟窗口已经消耗时间；请求和快照都不能重新给予完整响应时间。
            shared.lock().unwrap().set_turn_countdown(3);
            let response = handler.handle_game_request(
                &mut room,
                first_position as u64 + 1,
                play_request(action, Vec::new(), Some(6), Some(0)),
            );
            assert_eq!(
                response_code(&response, first_position as u64 + 1, Routes::PLAY),
                Some(WsResponseCode::OK as i32)
            );
            assert!(!has_room_event(&response, WsCode::PLAY));
            assert!(!has_room_event(&response, WsCode::GAME_OVER));
            let state = shared.lock().unwrap();
            assert_eq!(state.turn_countdown(), 3);
            assert_eq!(state.wall, vec![36, 37]);
            assert_eq!(state.melds[&0][0].kind, ShenyangMahjongMeldKind::PENG);
            assert!(state.hands[&0].contains(&6));
            for viewer in 0..4 {
                let event = claim_for(&response, viewer as u64 + 1);
                let snapshot =
                    build_table_snapshot_event_with_configs(&state, viewer, &default_configs());
                assert_eq!(
                    serde_json::to_value(&event).unwrap(),
                    serde_json::to_value(snapshot.claim_window.unwrap()).unwrap()
                );
                assert!(event.is_rob_gang);
                assert_eq!(event.seconds, 3);
                if viewer == 3 - first_position {
                    assert_eq!(event.eligible_positions, vec![viewer as i32]);
                    assert_eq!(event.options.len(), 1);
                    assert_eq!(event.options[0].position, viewer as i32);
                    assert!(event.options[0].can_hu);
                    assert!(!event.options[0].can_peng);
                    assert!(!event.options[0].can_gang);
                    assert!(event.options[0].chi_options.is_empty());
                } else {
                    assert!(event.eligible_positions.is_empty());
                    assert!(event.options.is_empty());
                }
            }
            drop(state);
            let duplicate = handler.handle_game_request(
                &mut room,
                first_position as u64 + 1,
                play_request(action, Vec::new(), Some(6), Some(0)),
            );
            assert_eq!(
                response_code(&duplicate, first_position as u64 + 1, Routes::PLAY),
                Some(WsResponseCode::NO_PERMISSION as i32)
            );
            assert!(!has_room_event(&duplicate, WsCode::CLAIM_WINDOW));
            assert_eq!(shared.lock().unwrap().turn_countdown(), 3);
        }
    }
}

#[test]
fn rob_gang_competing_responses_settle_once_in_seat_order_or_draw_one_replacement() {
    for first_position in [1, 2] {
        for actions in [
            [ShenyangMahjongAction::HU, ShenyangMahjongAction::HU],
            [ShenyangMahjongAction::HU, ShenyangMahjongAction::PASS],
            [ShenyangMahjongAction::PASS, ShenyangMahjongAction::HU],
            [ShenyangMahjongAction::PASS, ShenyangMahjongAction::PASS],
        ] {
            let (mut room, mut handler, shared) = rob_gang_room();
            let mut winners = Vec::new();
            let mut resolved = Dispatch::default();
            for (index, (position, action)) in [first_position, 3 - first_position]
                .into_iter()
                .zip(actions)
                .enumerate()
            {
                if action == ShenyangMahjongAction::HU {
                    winners.push(position);
                }
                resolved = handler.handle_game_request(
                    &mut room,
                    position as u64 + 1,
                    play_request(action, Vec::new(), Some(6), Some(0)),
                );
                assert_eq!(
                    response_code(&resolved, position as u64 + 1, Routes::PLAY),
                    Some(WsResponseCode::OK as i32)
                );
                if index == 0 {
                    assert!(!has_room_event(&resolved, WsCode::GAME_OVER));
                    assert!(!has_room_event(&resolved, WsCode::PLAY));
                }
            }
            winners.sort_unstable();
            assert!(!has_room_event(&resolved, WsCode::CLAIM_WINDOW));
            let state = shared.lock().unwrap();
            assert!(state.claim_window.is_none());
            let source_plays = plays_for(&resolved, 1);
            if winners.is_empty() {
                assert!(state.settlement.is_none());
                assert_eq!(state.wall_count(), 1);
                assert_eq!(state.melds[&0][0].kind, ShenyangMahjongMeldKind::GANG);
                assert_eq!(state.melds[&0][0].tiles, vec![6, 6, 6, 6]);
                assert_eq!(
                    source_plays
                        .iter()
                        .filter(|play| play.action == ShenyangMahjongAction::GANG)
                        .count(),
                    1
                );
                let draws = source_plays
                    .iter()
                    .filter(|play| play.action == ShenyangMahjongAction::DRAW)
                    .collect::<Vec<_>>();
                assert_eq!(draws.len(), 1);
                assert_eq!(draws[0].tiles.len(), 1);
                assert_eq!(state.last_drawn_tile, Some(draws[0].tiles[0]));
            } else {
                let settlement = state.settlement.as_ref().unwrap();
                assert_eq!(settlement.winner_positions, winners);
                assert!(settlement.is_reverse_win);
                assert_eq!(state.wall, vec![36, 37]);
                assert_eq!(state.melds[&0][0].kind, ShenyangMahjongMeldKind::PENG);
                assert_eq!(source_plays.len(), winners.len());
                assert!(
                    source_plays
                        .iter()
                        .all(|play| play.action == ShenyangMahjongAction::HU)
                );
                assert_eq!(
                    source_plays
                        .iter()
                        .map(|play| play.position as usize)
                        .collect::<Vec<_>>(),
                    winners
                );
            }
            assert!(!state.hands[&0].contains(&6));
            let wall = state.wall.clone();
            drop(state);
            let duplicate = handler.handle_game_request(
                &mut room,
                first_position as u64 + 1,
                play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(6), Some(0)),
            );
            assert_eq!(
                response_code(&duplicate, first_position as u64 + 1, Routes::PLAY),
                Some(WsResponseCode::NO_PERMISSION as i32)
            );
            assert!(!has_room_event(&duplicate, WsCode::PLAY));
            assert!(!has_room_event(&duplicate, WsCode::GAME_OVER));
            assert_eq!(shared.lock().unwrap().wall, wall);
        }
    }
}
