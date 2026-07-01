use std::{collections::HashMap, sync::Arc};

use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, GameId, Routes, TexasHoldEmAction, TexasHoldEmPhase, WsCode, WsResponseCode,
    games::texas_hold_em::{
        WsTexasHoldEmActionEvent, WsTexasHoldEmDealEvent, WsTexasHoldEmPlayRequest,
        WsTexasHoldEmPublicCardsEvent, WsTexasHoldEmSettlementEvent, WsTexasHoldEmSettlementPlayer,
        WsTexasHoldEmTurnEvent,
    },
};
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RoomService, SessionId,
    game_state::SharedGameState,
};

use crate::{
    game_setting::build_texas_hold_em_settings,
    game_state::{TexasHoldEmGameState, TexasHoldEmStateHandle},
};

type StateRegistry = Arc<std::sync::Mutex<HashMap<String, TexasHoldEmStateHandle>>>;

pub struct TexasHoldEmGameHandler {
    states: StateRegistry,
}

impl Default for TexasHoldEmGameHandler {
    fn default() -> Self {
        Self {
            states: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl TexasHoldEmGameHandler {
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
        let initial_chips = configs.get("initial_chips").copied().unwrap_or(1000);
        let small_blind = configs.get("small_blind").copied().unwrap_or(5);
        let big_blind = configs
            .get("big_blind")
            .copied()
            .unwrap_or(10)
            .max(small_blind + 1);

        let mut state = TexasHoldEmGameState::from_common(Arc::clone(&common));
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
        let state = Arc::new(std::sync::Mutex::new(state));
        room_service.set_room_game_state(
            &room_key,
            Box::new(SharedGameState::from_common(Arc::clone(&common))),
        );
        self.states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&state));

        room_service.send_all(&room_key, WsCode::START as i32, json!({}), &mut dispatch);
        self.push_private_deals(&room_key, room_service, &state, &mut dispatch);
        self.push_turn_event(&room_key, room_service, &state, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
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

        let action_event = {
            let mut s = state.lock().unwrap();
            if s.phase == TexasHoldEmPhase::Settlement
                || s.current_position != position
                || s.folded.contains(&position)
                || s.all_in.contains(&position)
            {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let Some(event) = apply_action(&mut s, position, payload) else {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            s.set_action_received(true);
            event
        };

        let mut dispatch = Dispatch::default();
        room_service.send_all(&room_key, WsCode::PLAY as i32, action_event, &mut dispatch);
        self.advance_after_action(&room_key, room_service, &state, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
        dispatch
    }

    fn state(&self, room_key: &str) -> Option<TexasHoldEmStateHandle> {
        self.states.lock().unwrap().get(room_key).cloned()
    }

    fn push_private_deals(
        &self,
        room_key: &str,
        room_service: &RoomService,
        state: &TexasHoldEmStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let s = state.lock().unwrap();
        for (session_id, _, position, _) in room_service.get_room_members(room_key) {
            let payload = WsTexasHoldEmDealEvent {
                my_cards: s.hands.get(&position).cloned().unwrap_or_default(),
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
        state: &TexasHoldEmStateHandle,
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
        state: &TexasHoldEmStateHandle,
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
            },
            dispatch,
        );
    }

    fn advance_after_action(
        &self,
        room_key: &str,
        room_service: &mut RoomService,
        state: &TexasHoldEmStateHandle,
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
            room_service.send_all(room_key, WsCode::GAME_OVER as i32, settlement, dispatch);
            self.states.lock().unwrap().remove(room_key);
            room_service.clear_room_game_state(room_key);
            return;
        }

        self.push_public_cards(room_key, room_service, state, dispatch);
        self.push_turn_event(room_key, room_service, state, dispatch);
    }
}

fn apply_action(
    s: &mut TexasHoldEmGameState,
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

fn settle_hand(state: &TexasHoldEmStateHandle) -> WsTexasHoldEmSettlementEvent {
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

impl GameHandler for TexasHoldEmGameHandler {
    fn game_id(&self) -> GameId {
        GameId::TEXAS_HOLD_EM
    }

    fn build_game_state(&self) -> Box<dyn ws_common::game_state::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_texas_hold_em_settings()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use share_type_public::WsJoinRequest;

    fn join_request(name: &str) -> ClientRequest {
        ClientRequest {
            route: Routes::JOIN as i32,
            data: serde_json::to_value(WsJoinRequest {
                name: name.to_string(),
                password: "room".to_string(),
                game_id: GameId::TEXAS_HOLD_EM,
                avatar_url: String::new(),
            })
            .unwrap(),
        }
    }

    #[test]
    fn texas_room_allows_two_to_eight_players() {
        let handler = TexasHoldEmGameHandler::default();
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

    #[test]
    fn start_deals_private_cards() {
        let handler = TexasHoldEmGameHandler::default();
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
    }
}
