use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, GameId, Routes, TexasHoldEmAction, TexasHoldEmAutoStrategy, TexasHoldEmPhase,
    WsCode, WsResponseCode,
    games::texas_hold_em::{
        WsTexasHoldEmActionEvent, WsTexasHoldEmAutoStrategyRequest, WsTexasHoldEmDealEvent,
        WsTexasHoldEmPlayRequest, WsTexasHoldEmPublicCardsEvent, WsTexasHoldEmPublicHoleCards,
        WsTexasHoldEmSettlementEvent, WsTexasHoldEmSettlementPlayer, WsTexasHoldEmTurnEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RoomService, SessionId,
    SessionSenders, game_state::SharedGameState, to_text_message,
};

use crate::{
    game_setting::build_holdem_settings,
    game_state::{HoldemGameState, HoldemStateHandle},
    poker_variant::{STANDARD_TEXAS, accepts_poker_game_id, variant_for_game_id},
};

pub struct HoldemGameHandler {
    states: StateRegistry,
    auto_strategies:
        Arc<std::sync::Mutex<HashMap<String, HashMap<usize, TexasHoldEmAutoStrategy>>>>,
}

type StateRegistry = Arc<std::sync::Mutex<HashMap<String, HoldemStateHandle>>>;

fn apply_action(
    s: &mut HoldemGameState,
    position: usize,
    payload: WsTexasHoldEmPlayRequest,
) -> Option<WsTexasHoldEmActionEvent> {
    let before_bet = s.bet_of(position);
    let before_pot = s.pot;
    let call_amount = s.call_amount(position);

    match payload.action {
        TexasHoldEmAction::FOLD => {
            s.folded.insert(position);
            s.acted.insert(position);
        }
        TexasHoldEmAction::CHECK => {
            if call_amount != 0 {
                return None;
            }
            s.acted.insert(position);
        }
        TexasHoldEmAction::CALL => {
            if call_amount <= 0 {
                return None;
            }
            s.commit(position, call_amount);
            s.acted.insert(position);
        }
        TexasHoldEmAction::BET => {
            if s.current_bet != 0 || payload.amount < s.big_blind {
                return None;
            }
            let paid = s.commit(position, payload.amount);
            if paid <= 0 {
                return None;
            }
            s.current_bet = s.bet_of(position);
            s.min_raise = paid.max(s.big_blind);
            s.acted.clear();
            s.acted.insert(position);
        }
        TexasHoldEmAction::RAISE => {
            if s.current_bet == 0 || payload.amount < s.min_raise {
                return None;
            }
            let paid = s.commit(position, call_amount + payload.amount);
            if paid <= call_amount {
                return None;
            }
            let new_bet = s.bet_of(position);
            if new_bet <= s.current_bet {
                return None;
            }
            s.min_raise = (new_bet - s.current_bet).max(s.big_blind);
            s.current_bet = new_bet;
            s.acted.clear();
            s.acted.insert(position);
        }
        TexasHoldEmAction::ALL_IN => {
            let paid = s.commit(position, s.chip_count(position));
            if paid <= 0 {
                return None;
            }
            let new_bet = s.bet_of(position);
            if new_bet > s.current_bet {
                s.min_raise = (new_bet - s.current_bet).max(s.big_blind);
                s.current_bet = new_bet;
                s.acted.clear();
            }
            s.acted.insert(position);
        }
    }

    Some(WsTexasHoldEmActionEvent {
        name: s.player_name(position),
        position: position as i32,
        action: payload.action,
        amount: s.pot - before_pot,
        committed: s.bet_of(position) - before_bet,
        current_bet: s.current_bet,
        pot: s.pot,
        chips: s.chip_count(position),
    })
}

async fn deliver_dispatch(dispatch: Dispatch, senders: &SessionSenders) {
    let mut frames = Vec::with_capacity(dispatch.messages.len());
    for message in dispatch.messages {
        if let Ok(frame) = to_text_message(&message.payload) {
            frames.push((message.recipient, frame));
        }
    }

    let senders = senders.lock().await;
    for (recipient, frame) in frames {
        if let Some(tx) = senders.get(&recipient) {
            let _ = tx.send(frame);
        }
    }
}

fn settle_hand(state: &HoldemStateHandle) -> WsTexasHoldEmSettlementEvent {
    let mut s = state.lock().unwrap();
    s.phase = TexasHoldEmPhase::Settlement;
    let contenders = s.active_not_folded_positions();
    let winners = if contenders.len() == 1 {
        contenders
    } else {
        let mut best = None;
        let mut winners = Vec::new();
        for position in contenders {
            let Some(hand) = s.evaluated_hand(position) else {
                continue;
            };
            if best.as_ref().is_none_or(|current| hand > *current) {
                best = Some(hand);
                winners.clear();
                winners.push(position);
            } else if best.as_ref().is_some_and(|current| hand == *current) {
                winners.push(position);
            }
        }
        winners
    };

    if !winners.is_empty() {
        let share = s.pot / winners.len() as i32;
        let remainder = s.pot % winners.len() as i32;
        for (idx, winner) in winners.iter().enumerate() {
            let extra = if idx == 0 { remainder } else { 0 };
            *s.chips.entry(*winner).or_default() += share + extra;
        }
    }

    let mut players = Vec::new();
    for position in s.active_positions() {
        let evaluated = s.evaluated_hand(position);
        players.push(WsTexasHoldEmSettlementPlayer {
            position: position as i32,
            name: s.player_name(position),
            cards: s.hands.get(&position).cloned().unwrap_or_default(),
            open_cards: s
                .hands
                .get(&position)
                .map(|cards| s.variant.public_hole_cards(cards))
                .unwrap_or_default(),
            folded: s.folded.contains(&position),
            chips: s.chip_count(position),
            hand_rank: evaluated.as_ref().map(|hand| hand.category).unwrap_or(-1),
            hand_name: evaluated
                .map(|hand| hand.name.to_string())
                .unwrap_or_default(),
        });
    }

    WsTexasHoldEmSettlementEvent {
        winners: winners
            .into_iter()
            .map(|position| position as i32)
            .collect(),
        pot: s.pot,
        public_cards: s.public_cards.clone(),
        players,
    }
}

impl HoldemGameHandler {
    fn advance_after_action(
        &self,
        room_key: &str,
        room_service: &mut RoomService,
        state: &HoldemStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let mut should_settle = false;
        {
            let mut s = state.lock().unwrap();
            if s.is_hand_over_by_folds() {
                should_settle = true;
            } else if s.is_round_complete() {
                loop {
                    let phase = s.reveal_next_phase();
                    if phase == TexasHoldEmPhase::Settlement {
                        should_settle = true;
                        break;
                    }
                    if s.next_action_position(s.dealer_position).is_some() {
                        break;
                    }
                }
            } else if let Some(next) = s.next_action_position(s.current_position) {
                s.current_position = next;
            }
        }

        if should_settle {
            let settlement = settle_hand(state);
            crate::official::settle_round(room_service, room_key);
            room_service.send_all(room_key, WsCode::GAME_OVER as i32, settlement, dispatch);
            self.states.lock().unwrap().remove(room_key);
            room_service.clear_room_game_state(room_key);
            return;
        }

        self.push_public_cards(room_key, room_service, state, dispatch);
        self.push_turn_event(room_key, room_service, state, dispatch);
    }

    fn apply_position_action(
        &self,
        room_key: &str,
        room_service: &mut RoomService,
        state: &HoldemStateHandle,
        position: usize,
        payload: WsTexasHoldEmPlayRequest,
        dispatch: &mut Dispatch,
    ) -> bool {
        let action_event = {
            let mut s = state.lock().unwrap();
            if s.phase == TexasHoldEmPhase::Settlement
                || s.current_position != position
                || s.folded.contains(&position)
                || s.all_in.contains(&position)
            {
                return false;
            }
            let Some(event) = apply_action(&mut s, position, payload) else {
                return false;
            };
            s.set_action_received(true);
            let play_time = room_service
                .get_room_configs(room_key)
                .unwrap_or_default()
                .get("play_time")
                .copied()
                .unwrap_or(20)
                .max(1) as u32;
            s.set_turn_countdown(play_time);
            event
        };

        room_service.send_all(room_key, WsCode::PLAY as i32, action_event, dispatch);
        self.advance_after_action(room_key, room_service, state, dispatch);
        true
    }

    fn auto_payload_for(
        strategy: TexasHoldEmAutoStrategy,
        call_amount: i32,
    ) -> WsTexasHoldEmPlayRequest {
        let action = match strategy {
            TexasHoldEmAutoStrategy::CHECK_CALL if call_amount > 0 => TexasHoldEmAction::CALL,
            TexasHoldEmAutoStrategy::CHECK_CALL => TexasHoldEmAction::CHECK,
            TexasHoldEmAutoStrategy::CHECK_FOLD if call_amount > 0 => TexasHoldEmAction::FOLD,
            TexasHoldEmAutoStrategy::CHECK_FOLD => TexasHoldEmAction::CHECK,
        };
        WsTexasHoldEmPlayRequest { action, amount: 0 }
    }

    fn auto_tick(
        &self,
        room_service: &mut RoomService,
        room_key: &str,
        state: &HoldemStateHandle,
        dispatch: &mut Dispatch,
    ) {
        if room_service.is_room_paused(room_key) {
            return;
        }

        let mut should_push_turn = false;
        let Some((position, call_amount, is_ai_position, should_auto)) = ({
            let mut s = state.lock().unwrap();
            if s.phase == TexasHoldEmPhase::Settlement {
                None
            } else {
                let position = s.current_position;
                let base = s.base.lock().unwrap();
                let is_ai_position = base.is_ai_position(position);
                let should_auto = is_ai_position || base.is_away(position);
                drop(base);
                if should_auto {
                    Some((
                        position,
                        s.call_amount(position),
                        is_ai_position,
                        should_auto,
                    ))
                } else if s.turn_countdown() > 0 {
                    let countdown = s.turn_countdown();
                    s.set_turn_countdown(countdown - 1);
                    should_push_turn = true;
                    None
                } else {
                    Some((
                        position,
                        s.call_amount(position),
                        is_ai_position,
                        should_auto,
                    ))
                }
            }
        }) else {
            if should_push_turn {
                self.push_turn_event(room_key, room_service, state, dispatch);
            }
            return;
        };

        if !should_auto {
            let base = state.lock().unwrap().base.clone();
            base.lock().unwrap().mark_away(position);
            room_service.send_all(
                room_key,
                WsCode::AWAY as i32,
                share_type_public::WsPositionEvent {
                    position: position as i32,
                },
                dispatch,
            );
        }
        let payload = if is_ai_position {
            let s = state.lock().unwrap();
            crate::ai::decide(&s, position)
        } else {
            let strategy = self
                .auto_strategies
                .lock()
                .unwrap()
                .get(room_key)
                .and_then(|strategies| strategies.get(&position).copied())
                .unwrap_or(TexasHoldEmAutoStrategy::CHECK_FOLD);
            Self::auto_payload_for(strategy, call_amount)
        };
        let _ =
            self.apply_position_action(room_key, room_service, state, position, payload, dispatch);
    }

    fn handle_auto_strategy(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::AUTO_STRATEGY as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::AUTO_STRATEGY as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Ok(payload) = RoomService::parse::<WsTexasHoldEmAutoStrategyRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::AUTO_STRATEGY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };

        self.auto_strategies
            .lock()
            .unwrap()
            .entry(room_key)
            .or_default()
            .insert(position, payload.strategy);

        let mut dispatch = Dispatch::default();
        room_service.push_ok_response(&mut dispatch, session_id, Routes::AUTO_STRATEGY as i32);
        dispatch
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
        let Ok(payload) = RoomService::parse::<WsTexasHoldEmPlayRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(state) = self.state(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        let mut dispatch = Dispatch::default();
        if !self.apply_position_action(
            &room_key,
            room_service,
            &state,
            position,
            payload,
            &mut dispatch,
        ) {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
        dispatch
    }

    fn handle_start(&self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
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
        if !room_service.ensure_in_room(session_id, Routes::START as i32, &mut dispatch) {
            return dispatch;
        }
        if !room_service.room_ready_to_start(session_id) {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Some(common) = room_service.get_room_common_state_handle(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        let configs = room_service.get_room_configs(&room_key).unwrap_or_default();
        let variant = room_service
            .room_game_id(&room_key)
            .and_then(variant_for_game_id)
            .unwrap_or(STANDARD_TEXAS);
        let initial_chips = configs.get("initial_chips").copied().unwrap_or(1000);
        let small_blind = configs.get("small_blind").copied().unwrap_or(5);
        let big_blind = configs
            .get("big_blind")
            .copied()
            .unwrap_or(10)
            .max(small_blind + 1);
        let mut state = HoldemGameState::from_common_with_variant(Arc::clone(&common), variant);
        if state
            .deal_new_hand(initial_chips, small_blind, big_blind)
            .is_err()
        {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        state.set_turn_countdown(1);
        let state = Arc::new(std::sync::Mutex::new(state));
        room_service.set_room_game_state(
            &room_key,
            Box::new(SharedGameState::from_common(Arc::clone(&common))),
        );
        self.states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&state));

        if let Some(game_id) = room_service.room_game_id(&room_key) {
            crate::official::create_match(room_service, &room_key, game_id);
        }
        room_service.send_all(&room_key, WsCode::START as i32, json!({}), &mut dispatch);
        self.push_private_deals(&room_key, room_service, &state, &mut dispatch);
        self.push_turn_event(&room_key, room_service, &state, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
        dispatch
    }

    fn push_private_deals(
        &self,
        room_key: &str,
        room_service: &RoomService,
        state: &HoldemStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let s = state.lock().unwrap();
        let public_hole_cards: Vec<_> = s
            .active_positions()
            .into_iter()
            .filter_map(|position| {
                let cards = s
                    .hands
                    .get(&position)
                    .map(|cards| s.variant.public_hole_cards(cards))
                    .unwrap_or_default();
                (!cards.is_empty()).then_some(WsTexasHoldEmPublicHoleCards {
                    position: position as i32,
                    cards,
                })
            })
            .collect();
        for (session_id, _, position, _) in room_service.get_room_members(room_key) {
            let payload = WsTexasHoldEmDealEvent {
                my_cards: s.hands.get(&position).cloned().unwrap_or_default(),
                open_cards: s
                    .hands
                    .get(&position)
                    .map(|cards| s.variant.public_hole_cards(cards))
                    .unwrap_or_default(),
                public_hole_cards: public_hole_cards.clone(),
                dealer_position: s.dealer_position as i32,
                small_blind_position: s.small_blind_position as i32,
                big_blind_position: s.big_blind_position as i32,
                chips: s.chip_count(position),
                pot: s.pot,
            };
            dispatch.messages.push(Delivery {
                recipient: session_id,
                payload: OutboundPayload::Event(CommonEvent {
                    code: WsCode::DEAL as i32,
                    data: serde_json::to_value(payload).unwrap_or(Value::Null),
                }),
            });
        }
    }

    fn push_public_cards(
        &self,
        room_key: &str,
        room_service: &RoomService,
        state: &HoldemStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let s = state.lock().unwrap();
        room_service.send_all(
            room_key,
            WsCode::DEAL_OPEN_CARDS as i32,
            WsTexasHoldEmPublicCardsEvent {
                phase: s.phase,
                cards: s.public_cards.clone(),
                pot: s.pot,
            },
            dispatch,
        );
    }

    fn push_turn_event(
        &self,
        room_key: &str,
        room_service: &RoomService,
        state: &HoldemStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let s = state.lock().unwrap();
        room_service.send_all(
            room_key,
            WsCode::CHANGE_DEAL as i32,
            WsTexasHoldEmTurnEvent {
                position: s.current_position as i32,
                phase: s.phase,
                call_amount: s.call_amount(s.current_position),
                min_raise: s.min_raise,
                current_bet: s.current_bet,
                pot: s.pot,
                turn_countdown: s.turn_countdown() as i32,
            },
            dispatch,
        );
    }

    fn state(&self, room_key: &str) -> Option<HoldemStateHandle> {
        self.states.lock().unwrap().get(room_key).cloned()
    }
}

impl Default for HoldemGameHandler {
    fn default() -> Self {
        Self {
            states: Arc::new(std::sync::Mutex::new(HashMap::new())),
            auto_strategies: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl GameHandler for HoldemGameHandler {
    fn accepts_game_id(&self, game_id: GameId) -> bool {
        accepts_poker_game_id(game_id)
    }

    fn build_game_state(&self) -> Box<dyn ws_common::game_state::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_holdem_settings()
    }

    fn game_id(&self) -> GameId {
        GameId::TEXAS_HOLD_EM
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
            r if r == Routes::AUTO_STRATEGY as i32 => {
                self.handle_auto_strategy(room_service, session_id, request.data)
            }
            _ => {
                room_service.error_response(session_id, request.route, WsResponseCode::NOT_IN_RANGE)
            }
        }
    }

    fn set_context(&mut self, senders: SessionSenders, room_service: Arc<Mutex<RoomService>>) {
        let states = Arc::clone(&self.states);
        let auto_strategies = Arc::clone(&self.auto_strategies);
        tokio::spawn(async move {
            let handler = HoldemGameHandler {
                states,
                auto_strategies,
            };
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            loop {
                ticker.tick().await;
                let rooms: Vec<(String, HoldemStateHandle)> = handler
                    .states
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(room_key, state)| (room_key.clone(), Arc::clone(state)))
                    .collect();
                if rooms.is_empty() {
                    continue;
                }
                let mut room = room_service.lock().await;
                let mut dispatch = Dispatch::default();
                for (room_key, state) in rooms {
                    handler.auto_tick(&mut room, &room_key, &state, &mut dispatch);
                }
                if dispatch.messages.is_empty() {
                    continue;
                }
                drop(room);
                deliver_dispatch(dispatch, &senders).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use share_type_public::WsJoinRequest;

    #[test]
    fn ai_positions_use_smart_ai_instead_of_stale_strategy() {
        let handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        room.handle_common_request(
            1,
            &ClientRequest {
                route: Routes::ADD_AI as i32,
                data: serde_json::json!({"count": 1}),
            },
            handler.game_id(),
            || handler.build_room_settings(),
        );
        let common = room
            .get_room_common_state_handle("room")
            .expect("room common state");
        let state = Arc::new(std::sync::Mutex::new(
            HoldemGameState::from_common_with_variant(common, STANDARD_TEXAS),
        ));
        {
            let mut s = state.lock().unwrap();
            s.current_position = 2;
            s.current_bet = 20;
            s.commit(0, 20);
            s.set_turn_countdown(20);
        }
        handler
            .auto_strategies
            .lock()
            .unwrap()
            .entry("room".to_string())
            .or_default()
            .insert(2, TexasHoldEmAutoStrategy::CHECK_FOLD);

        let mut dispatch = Dispatch::default();
        handler.auto_tick(&mut room, "room", &state, &mut dispatch);

        let action = dispatch.messages.iter().find_map(|message| {
            let OutboundPayload::Event(event) = &message.payload else {
                return None;
            };
            (event.code == WsCode::PLAY as i32)
                .then(|| {
                    serde_json::from_value::<WsTexasHoldEmActionEvent>(event.data.clone()).ok()
                })
                .flatten()
        });
        assert!(
            action.is_some(),
            "AI position should produce a PLAY event via smart AI module"
        );
    }

    #[test]
    fn auto_strategy_maps_to_check_fold_or_check_call() {
        assert_eq!(
            HoldemGameHandler::auto_payload_for(TexasHoldEmAutoStrategy::CHECK_FOLD, 0).action,
            TexasHoldEmAction::CHECK
        );
        assert_eq!(
            HoldemGameHandler::auto_payload_for(TexasHoldEmAutoStrategy::CHECK_FOLD, 10).action,
            TexasHoldEmAction::FOLD
        );
        assert_eq!(
            HoldemGameHandler::auto_payload_for(TexasHoldEmAutoStrategy::CHECK_CALL, 0).action,
            TexasHoldEmAction::CHECK
        );
        assert_eq!(
            HoldemGameHandler::auto_payload_for(TexasHoldEmAutoStrategy::CHECK_CALL, 10).action,
            TexasHoldEmAction::CALL
        );
    }

    #[test]
    fn auto_strategy_response_is_private_to_requester() {
        let mut handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }

        let dispatch = handler.handle_game_request(
            &mut room,
            1,
            ClientRequest {
                route: Routes::AUTO_STRATEGY as i32,
                data: serde_json::to_value(WsTexasHoldEmAutoStrategyRequest {
                    strategy: TexasHoldEmAutoStrategy::CHECK_CALL,
                })
                .unwrap(),
            },
        );

        assert_eq!(dispatch.messages.len(), 1);
        assert_eq!(dispatch.messages[0].recipient, 1);
        assert!(matches!(
            &dispatch.messages[0].payload,
            OutboundPayload::Response(ws_common::RequestResponse::WithoutData(response))
                if response.route == Routes::AUTO_STRATEGY as i32
                    && matches!(response.code, WsResponseCode::OK)
        ));
        assert!(
            dispatch
                .messages
                .iter()
                .all(|message| !matches!(&message.payload, OutboundPayload::Event(_)))
        );
        assert_eq!(
            handler
                .auto_strategies
                .lock()
                .unwrap()
                .get("room")
                .and_then(|strategies| strategies.get(&0).copied()),
            Some(TexasHoldEmAutoStrategy::CHECK_CALL)
        );
    }

    #[test]
    fn holdem_room_allows_two_to_eight_players() {
        let handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=8 {
            let dispatch = room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
            assert!(dispatch.is_some());
        }
        let denied = room
            .handle_common_request(9, &join_request("u9"), handler.game_id(), || {
                handler.build_room_settings()
            })
            .unwrap();
        assert_eq!(denied.messages.len(), 1);
    }

    fn join_request(name: &str) -> ClientRequest {
        join_request_for_game(name, GameId::TEXAS_HOLD_EM)
    }

    fn join_request_for_game(name: &str, game_id: GameId) -> ClientRequest {
        ClientRequest {
            route: Routes::JOIN as i32,
            data: serde_json::to_value(WsJoinRequest {
                name: name.to_string(),
                password: "room".to_string(),
                game_id,
                session_id: String::new(),
                avatar_url: String::new(),
            })
            .unwrap(),
        }
    }

    #[test]
    fn omaha_deals_four_hole_cards() {
        let deals = started_deals_for(GameId::OMAHA_HOLD_EM);
        assert_eq!(deals.len(), 2);
        for deal in deals {
            assert_eq!(deal.my_cards.len(), 4);
            assert_eq!(deal.open_cards.len(), 0);
            assert!(deal.public_hole_cards.is_empty());
        }
    }

    #[test]
    fn open_hold_em_deals_three_hole_cards_with_one_open_card() {
        let deals = started_deals_for(GameId::OPEN_HOLD_EM);
        assert_eq!(deals.len(), 2);
        for deal in deals {
            assert_eq!(deal.my_cards.len(), 3);
            assert_eq!(deal.open_cards.len(), 1);
            assert_eq!(deal.open_cards[0], deal.my_cards[2]);
            assert_eq!(deal.public_hole_cards.len(), 2);
            assert!(
                deal.public_hole_cards
                    .iter()
                    .all(|item| item.cards.len() == 1)
            );
        }
    }

    #[test]
    fn short_deck_removes_low_cards() {
        let deals = started_deals_for(GameId::SHORT_DECK_HOLD_EM);
        assert_eq!(deals.len(), 2);
        for deal in deals {
            assert_eq!(deal.my_cards.len(), 2);
            assert!(
                deal.my_cards
                    .iter()
                    .all(|card| crate::hand_evaluator::card_rank(*card) >= 6)
            );
        }
    }

    #[test]
    fn start_deals_private_cards() {
        let handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let dispatch = handler.handle_start(&mut room, 1);
        let private_deals = dispatch
            .messages
            .iter()
            .filter(|message| {
                matches!(
                    &message.payload,
                    OutboundPayload::Event(event) if event.code == WsCode::DEAL as i32
                )
            })
            .count();
        assert_eq!(private_deals, 2);
        for message in dispatch.messages.iter().filter(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Event(event) if event.code == WsCode::DEAL as i32
            )
        }) {
            let OutboundPayload::Event(event) = &message.payload else {
                unreachable!();
            };
            let deal: WsTexasHoldEmDealEvent = serde_json::from_value(event.data.clone()).unwrap();
            assert_eq!(deal.my_cards.len(), 2);
            assert_eq!(deal.open_cards.len(), 0);
        }
    }

    fn started_deals_for(game_id: GameId) -> Vec<WsTexasHoldEmDealEvent> {
        let handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            room.handle_common_request_with_game_acceptance(
                session_id,
                &join_request_for_game(&format!("u{session_id}"), game_id),
                accepts_poker_game_id,
                || handler.build_room_settings(),
            );
        }
        let dispatch = handler.handle_start(&mut room, 1);
        dispatch
            .messages
            .iter()
            .filter_map(|message| {
                let OutboundPayload::Event(event) = &message.payload else {
                    return None;
                };
                (event.code == WsCode::DEAL as i32)
                    .then(|| serde_json::from_value(event.data.clone()).unwrap())
            })
            .collect()
    }
}
