use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, GameId, Routes, TexasHoldEmAction, TexasHoldEmAutoStrategy, TexasHoldEmPhase,
    WsCode, WsResponseCode,
    games::texas_hold_em::{
        WsTexasHoldEmActionEvent, WsTexasHoldEmAutoStrategyRequest, WsTexasHoldEmDealEvent,
        WsTexasHoldEmPlayRequest, WsTexasHoldEmPlayerSnapshot, WsTexasHoldEmPublicCardsEvent,
        WsTexasHoldEmPublicHoleCards, WsTexasHoldEmSettlementEvent, WsTexasHoldEmSettlementPlayer,
        WsTexasHoldEmTableSnapshotEvent, WsTexasHoldEmTurnEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{
    ClientRequest, CommonGameState, Delivery, Dispatch, GameHandler, GameState, OutboundPayload,
    RequestResponse, RoomService, SessionId, SessionSenders, SharedGameState, to_text_message,
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

/// Room-facing state used while a hand is active.  Human JOIN remains open,
/// while settings, AI management, and seat swaps stay locked.
struct ActiveHoldemRoomState {
    common: Arc<std::sync::Mutex<CommonGameState>>,
    hand_positions: HashSet<usize>,
}

impl GameState for ActiveHoldemRoomState {
    fn can_accept_players(&self) -> bool {
        false
    }

    fn can_join_players(&self) -> bool {
        true
    }

    fn position_reserved_for_join(&self, position: usize) -> bool {
        self.hand_positions.contains(&position)
    }

    fn shared_common_state(&self) -> Arc<std::sync::Mutex<CommonGameState>> {
        Arc::clone(&self.common)
    }
}

fn join_succeeded(dispatch: &Dispatch, session_id: SessionId) -> bool {
    dispatch.messages.iter().any(|message| {
        message.recipient == session_id
            && matches!(
                &message.payload,
                OutboundPayload::Response(RequestResponse::WithData(response))
                    if response.route == Routes::JOIN as i32
                        && response.code as i32 == WsResponseCode::JOINED as i32
            )
    })
}

fn apply_action(
    s: &mut HoldemGameState,
    position: usize,
    payload: WsTexasHoldEmPlayRequest,
) -> Option<WsTexasHoldEmActionEvent> {
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
        committed: s.bet_of(position),
        current_bet: s.current_bet,
        pot: s.pot,
        chips: s.chip_count(position),
        folded: s.folded.contains(&position),
        all_in: s.all_in.contains(&position),
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
            room_service.broadcast(room_key, WsCode::GAME_OVER as i32, settlement, dispatch);
            let common = Self::common_state(state);
            self.remove_registered_state_if_same(room_key, state);
            room_service.clear_room_game_state_if_same(room_key, &common);
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
                .room_configs(room_key)
                .unwrap_or_default()
                .get("play_time")
                .copied()
                .unwrap_or(20)
                .max(1) as u32;
            s.set_turn_countdown(play_time);
            event
        };

        room_service.broadcast(room_key, WsCode::PLAY as i32, action_event, dispatch);
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
        if !Self::state_matches_room(room_service, room_key, state)
            || Self::state_stop_requested(state)
        {
            return;
        }
        if room_service.is_room_paused(room_key) {
            return;
        }

        let mut should_push_turn = false;
        let Some((position, call_amount, is_ai_position, should_auto, player_left)) = ({
            let mut s = state.lock().unwrap();
            if s.phase == TexasHoldEmPhase::Settlement {
                None
            } else {
                let position = s.current_position;
                let base = s.base.lock().unwrap();
                let player_left = s.hand_players.get(&position).is_some_and(|hand_name| {
                    base.players
                        .get(&position)
                        .is_none_or(|(_, room_name)| room_name != hand_name)
                });
                let is_ai_position = base.is_ai_position(position);
                let should_auto = player_left
                    || is_ai_position
                    || base.is_away(position)
                    || base.is_disconnected(position);
                drop(base);
                if should_auto {
                    Some((
                        position,
                        s.call_amount(position),
                        is_ai_position,
                        should_auto,
                        player_left,
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
                        player_left,
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
            room_service.broadcast(
                room_key,
                WsCode::AWAY as i32,
                share_type_public::WsPositionEvent {
                    position: position as i32,
                },
                dispatch,
            );
        }
        let payload = if player_left {
            WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::FOLD,
                amount: 0,
            }
        } else if is_ai_position {
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
        let Ok(payload) = RoomService::parse_payload::<WsTexasHoldEmAutoStrategyRequest>(data)
        else {
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
        let Ok(payload) = RoomService::parse_payload::<WsTexasHoldEmPlayRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        let session_name = room_service.session_name(session_id);
        if !state
            .lock()
            .unwrap()
            .is_hand_player(position, &session_name)
        {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }

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
        if !room_service.require_room_membership(session_id, Routes::START as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        if !room_service.room_is_ready_to_start(&room_key) {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        let Some(mut common) = room_service.room_common_state(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        if common.lock().unwrap().stop_requested() {
            let Some(next_common) = room_service.reset_room_common_state_for_new_game(&room_key)
            else {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            common = next_common;
        }
        if let Some(existing) = self.state(&room_key) {
            if Arc::ptr_eq(&Self::common_state(&existing), &common) {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            if self.remove_registered_state_if_same(&room_key, &existing) {
                self.auto_strategies.lock().unwrap().remove(&room_key);
            }
        }
        let configs = room_service.room_configs(&room_key).unwrap_or_default();
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
        let play_time = configs.get("play_time").copied().unwrap_or(20).max(1) as u32;
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
        state.set_turn_countdown(play_time);
        let hand_positions = state.hand_players.keys().copied().collect();
        let state = Arc::new(std::sync::Mutex::new(state));
        room_service.set_room_game_state(
            &room_key,
            Box::new(ActiveHoldemRoomState {
                common: Arc::clone(&common),
                hand_positions,
            }),
        );
        self.states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&state));

        if let Some(game_id) = room_service.room_game_id(&room_key) {
            crate::official::create_match(room_service, &room_key, game_id);
        }
        room_service.broadcast(&room_key, WsCode::START as i32, json!({}), &mut dispatch);
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
        let participant_positions = s
            .active_positions()
            .into_iter()
            .map(|position| position as i32)
            .collect::<Vec<_>>();
        for (session_id, _, position, _) in room_service.room_members(room_key) {
            let payload = WsTexasHoldEmDealEvent {
                my_cards: s.hands.get(&position).cloned().unwrap_or_default(),
                open_cards: s
                    .hands
                    .get(&position)
                    .map(|cards| s.variant.public_hole_cards(cards))
                    .unwrap_or_default(),
                participant_positions: participant_positions.clone(),
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
        room_service.broadcast(
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
        room_service.broadcast(
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

    fn push_table_snapshot(
        &self,
        room_service: &RoomService,
        session_id: SessionId,
        state: &HoldemStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let Some(self_position) = room_service.session_position(session_id) else {
            return;
        };
        let self_name = room_service.session_name(session_id);
        let s = state.lock().unwrap();
        let is_participating = s.is_hand_player(self_position, &self_name);
        let players = s
            .active_positions()
            .into_iter()
            .map(|position| {
                let open_cards = s
                    .hands
                    .get(&position)
                    .map(|cards| s.variant.public_hole_cards(cards))
                    .unwrap_or_default();
                WsTexasHoldEmPlayerSnapshot {
                    position: position as i32,
                    name: s.player_name(position),
                    chips: s.chip_count(position),
                    committed: s.bet_of(position),
                    folded: s.folded.contains(&position),
                    all_in: s.all_in.contains(&position),
                    open_cards,
                }
            })
            .collect();
        let payload = WsTexasHoldEmTableSnapshotEvent {
            self_position: self_position as i32,
            is_participating,
            phase: s.phase,
            public_cards: s.public_cards.clone(),
            players,
            my_cards: if is_participating {
                s.hands.get(&self_position).cloned().unwrap_or_default()
            } else {
                Vec::new()
            },
            dealer_position: s.dealer_position as i32,
            small_blind_position: s.small_blind_position as i32,
            big_blind_position: s.big_blind_position as i32,
            current_position: s.current_position as i32,
            call_amount: s.call_amount(s.current_position),
            min_raise: s.min_raise,
            current_bet: s.current_bet,
            pot: s.pot,
            turn_countdown: s.turn_countdown() as i32,
        };
        dispatch.messages.push(Delivery {
            recipient: session_id,
            payload: OutboundPayload::Event(CommonEvent {
                code: WsCode::TABLE_SNAPSHOT as i32,
                data: serde_json::to_value(payload).unwrap_or(Value::Null),
            }),
        });
    }

    fn state(&self, room_key: &str) -> Option<HoldemStateHandle> {
        self.states.lock().unwrap().get(room_key).cloned()
    }

    fn common_state(state: &HoldemStateHandle) -> Arc<std::sync::Mutex<CommonGameState>> {
        Arc::clone(&state.lock().unwrap().base)
    }

    fn state_matches_room(
        room_service: &RoomService,
        room_key: &str,
        state: &HoldemStateHandle,
    ) -> bool {
        let Some(current) = room_service.room_common_state(room_key) else {
            return false;
        };
        Arc::ptr_eq(&current, &Self::common_state(state))
    }

    fn state_stop_requested(state: &HoldemStateHandle) -> bool {
        let common = Self::common_state(state);
        common.lock().unwrap().stop_requested()
    }

    fn current_state(
        &self,
        room_service: &RoomService,
        room_key: &str,
    ) -> Option<HoldemStateHandle> {
        let state = self.state(room_key)?;
        (Self::state_matches_room(room_service, room_key, &state)
            && !Self::state_stop_requested(&state))
        .then_some(state)
    }

    fn remove_registered_state_if_same(
        &self,
        room_key: &str,
        expected: &HoldemStateHandle,
    ) -> bool {
        let mut states = self.states.lock().unwrap();
        let is_same = states
            .get(room_key)
            .is_some_and(|current| Arc::ptr_eq(current, expected));
        if is_same {
            states.remove(room_key);
        }
        is_same
    }

    fn prune_stopped_states(&self, room_service: &mut RoomService) {
        let states: Vec<_> = self
            .states
            .lock()
            .unwrap()
            .iter()
            .map(|(room_key, state)| (room_key.clone(), Arc::clone(state)))
            .collect();
        let stopped: Vec<_> = states
            .into_iter()
            .filter_map(|(room_key, state)| {
                Self::state_stop_requested(&state).then(|| {
                    let common = Self::common_state(&state);
                    (room_key, state, common)
                })
            })
            .collect();

        for (room_key, state, common) in stopped {
            if self.remove_registered_state_if_same(&room_key, &state) {
                self.auto_strategies.lock().unwrap().remove(&room_key);
            }
            room_service.clear_room_game_state_if_same(&room_key, &common);
        }
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

    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        if matches!(request.route, r if r == Routes::QUIT as i32 || r == Routes::DISBAND as i32) {
            self.prune_stopped_states(room_service);
        }
        if request.route != Routes::JOIN as i32 || !join_succeeded(dispatch, session_id) {
            return;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return;
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return;
        };
        self.push_table_snapshot(room_service, session_id, &state, dispatch);
    }

    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
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
                let mut stale_rooms = Vec::new();
                for (room_key, state) in rooms {
                    let common = HoldemGameHandler::common_state(&state);
                    let is_current = room
                        .room_common_state(&room_key)
                        .is_some_and(|current| Arc::ptr_eq(&current, &common));
                    let stop_requested = common.lock().unwrap().stop_requested();
                    if !is_current || stop_requested {
                        if stop_requested {
                            room.clear_room_game_state_if_same(&room_key, &common);
                        }
                        stale_rooms.push((room_key, state));
                        continue;
                    }
                    handler.auto_tick(&mut room, &room_key, &state, &mut dispatch);
                }
                if !stale_rooms.is_empty() {
                    for (room_key, state) in stale_rooms {
                        if handler.remove_registered_state_if_same(&room_key, &state) {
                            handler.auto_strategies.lock().unwrap().remove(&room_key);
                        }
                    }
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
    use share_type_public::{WsJoinRequest, WsJoinResponse};

    #[test]
    fn ai_positions_raise_premium_hand_instead_of_stale_strategy() {
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
        let common = room.room_common_state("room").expect("room common state");
        let state = Arc::new(std::sync::Mutex::new(
            HoldemGameState::from_common_with_variant(common, STANDARD_TEXAS),
        ));
        {
            let mut s = state.lock().unwrap();
            s.phase = TexasHoldEmPhase::PreFlop;
            s.current_position = 2;
            s.current_bet = 20;
            s.min_raise = 10;
            s.big_blind = 10;
            s.pot = 50;
            s.chips.insert(2, 1000);
            s.round_bets.insert(2, 0);
            s.hands.insert(2, vec![13, 26]);
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
        let action = action.expect("AI position should produce a PLAY event via smart AI module");
        assert_eq!(action.action, TexasHoldEmAction::RAISE);
        assert!(action.committed >= 30);
    }

    #[test]
    fn disconnected_current_player_is_auto_controlled_without_full_turn_timeout() {
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
        let _ = handler.handle_start(&mut room, 1);
        let state = handler.state("room").expect("active holdem state");
        let (position, session_id) = {
            let mut state = state.lock().unwrap();
            state.set_turn_countdown(20);
            let position = state.current_position;
            let session_id = state
                .base
                .lock()
                .unwrap()
                .players
                .get(&position)
                .expect("current player")
                .0;
            (position, session_id)
        };

        let _ = room.disconnect(session_id);
        assert!(room.room_exists("room"));
        let mut dispatch = Dispatch::default();
        handler.auto_tick(&mut room, "room", &state, &mut dispatch);

        assert!(dispatch.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Event(event) if event.code == WsCode::PLAY as i32
            )
        }));
        let state = state.lock().unwrap();
        assert!(state.folded.contains(&position) || state.acted.contains(&position));
        assert_ne!(state.turn_countdown(), 19);
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

    #[test]
    fn join_during_hand_spectates_then_participates_next_hand() {
        let mut handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            join_with_hook(
                &mut handler,
                &mut room,
                session_id,
                &format!("u{session_id}"),
            );
        }

        handler.handle_start(&mut room, 1);
        let state = handler.state("room").expect("active hand");
        assert_eq!(state.lock().unwrap().active_positions(), vec![0, 1]);

        let repeated_start = handler.handle_start(&mut room, 1);
        assert_response_code(
            &repeated_start,
            1,
            Routes::START,
            WsResponseCode::NO_PERMISSION,
        );

        let joined = join_with_hook(&mut handler, &mut room, 3, "u3");
        let response = join_response(&joined, 3);
        assert_eq!(response.self_position, 2);
        let snapshot = table_snapshot(&joined, 3);
        assert_eq!(snapshot.self_position, 2);
        assert!(!snapshot.is_participating);
        assert!(snapshot.my_cards.is_empty());
        assert_eq!(snapshot.players.len(), 2);
        assert_eq!(state.lock().unwrap().active_positions(), vec![0, 1]);

        let denied_play = handler.handle_play(
            &mut room,
            3,
            serde_json::to_value(WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::FOLD,
                amount: 0,
            })
            .unwrap(),
        );
        assert_response_code(&denied_play, 3, Routes::PLAY, WsResponseCode::NO_PERMISSION);

        let current_position = state.lock().unwrap().current_position;
        let current_session = room
            .room_members("room")
            .into_iter()
            .find_map(|(session_id, _, position, _)| {
                (position == current_position).then_some(session_id)
            })
            .expect("current player session");
        let settlement = handler.handle_play(
            &mut room,
            current_session,
            serde_json::to_value(WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::FOLD,
                amount: 0,
            })
            .unwrap(),
        );
        assert!(settlement.messages.iter().any(|message| {
            message.recipient == 3
                && matches!(
                    &message.payload,
                    OutboundPayload::Event(event) if event.code == WsCode::GAME_OVER as i32
                )
        }));
        assert!(handler.state("room").is_none());

        let next_hand = handler.handle_start(&mut room, 1);
        let spectator_deal = deal_for(&next_hand, 3);
        assert_eq!(spectator_deal.my_cards.len(), 2);
        assert_eq!(
            handler
                .state("room")
                .expect("next hand")
                .lock()
                .unwrap()
                .active_positions(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn current_hand_player_rejoin_receives_private_snapshot() {
        let mut handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            join_with_hook(
                &mut handler,
                &mut room,
                session_id,
                &format!("u{session_id}"),
            );
        }
        handler.handle_start(&mut room, 1);
        let expected_cards = handler
            .state("room")
            .expect("active hand")
            .lock()
            .unwrap()
            .hands
            .get(&1)
            .cloned()
            .expect("player cards");

        room.disconnect(2);
        let rejoined = join_with_hook(&mut handler, &mut room, 3, "u2");
        let response = join_response(&rejoined, 3);
        assert_eq!(response.self_position, 1);
        let snapshot = table_snapshot(&rejoined, 3);
        assert!(snapshot.is_participating);
        assert_eq!(snapshot.self_position, 1);
        assert_eq!(snapshot.my_cards, expected_cards);
    }

    #[test]
    fn quit_stops_active_hand_and_allows_a_fresh_start() {
        let mut handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            join_with_hook(
                &mut handler,
                &mut room,
                session_id,
                &format!("u{session_id}"),
            );
        }
        handler.handle_start(&mut room, 1);
        let state = handler.state("room").expect("active hand");
        state.lock().unwrap().set_turn_countdown(17);
        let common = HoldemGameHandler::common_state(&state);

        let request = ClientRequest {
            route: Routes::QUIT as i32,
            data: Value::Null,
        };
        let mut dispatch = room
            .handle_common_request(2, &request, handler.game_id(), || {
                handler.build_room_settings()
            })
            .expect("common QUIT dispatch");
        handler.after_common_request(&mut room, 2, &request, &mut dispatch);

        assert_eq!(state.lock().unwrap().turn_countdown(), 0);
        assert!(common.lock().unwrap().stop_requested());
        assert!(handler.state("room").is_none());

        let joined = join_with_hook(&mut handler, &mut room, 3, "u3");
        assert_eq!(join_response(&joined, 3).self_position, 1);
        let restarted = handler.handle_start(&mut room, 1);
        assert_response_code(&restarted, 1, Routes::START, WsResponseCode::OK);
        let restarted_state = handler.state("room").expect("restarted hand");
        assert!(!Arc::ptr_eq(
            &common,
            &HoldemGameHandler::common_state(&restarted_state)
        ));
    }

    #[test]
    fn last_disconnect_releases_name_and_stale_state_does_not_block_recreation() {
        let mut handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=2 {
            join_with_hook(
                &mut handler,
                &mut room,
                session_id,
                &format!("old{session_id}"),
            );
        }
        handler.handle_start(&mut room, 1);
        let old_state = handler.state("room").expect("old active hand");
        let old_common = HoldemGameHandler::common_state(&old_state);

        room.disconnect(1);
        assert!(room.room_exists("room"));
        room.disconnect(2);
        assert!(!room.room_exists("room"));
        assert!(old_common.lock().unwrap().stop_requested());

        let first_join = join_with_hook(&mut handler, &mut room, 3, "new3");
        assert!(first_join.messages.iter().all(|message| {
            !matches!(
                &message.payload,
                OutboundPayload::Event(event) if event.code == WsCode::TABLE_SNAPSHOT as i32
            )
        }));
        join_with_hook(&mut handler, &mut room, 4, "new4");
        let new_common = room
            .room_common_state("room")
            .expect("new room common state");
        assert!(!Arc::ptr_eq(&old_common, &new_common));

        let restarted = handler.handle_start(&mut room, 3);
        assert_response_code(&restarted, 3, Routes::START, WsResponseCode::OK);
        let current = handler.state("room").expect("new active hand");
        assert!(!Arc::ptr_eq(&old_state, &current));
        assert!(Arc::ptr_eq(
            &new_common,
            &HoldemGameHandler::common_state(&current)
        ));
    }

    #[test]
    fn stale_same_name_state_cannot_tick_a_different_room_instance() {
        let mut handler = HoldemGameHandler::default();
        let mut old_room = RoomService::default();
        for session_id in 1..=2 {
            join_with_hook(
                &mut handler,
                &mut old_room,
                session_id,
                &format!("old{session_id}"),
            );
        }
        handler.handle_start(&mut old_room, 1);
        let old_state = handler.state("room").expect("old active hand");
        old_state.lock().unwrap().set_turn_countdown(7);

        let mut new_room = RoomService::default();
        for session_id in 10..=11 {
            join_with_hook(
                &mut handler,
                &mut new_room,
                session_id,
                &format!("new{session_id}"),
            );
        }
        let mut dispatch = Dispatch::default();
        handler.auto_tick(&mut new_room, "room", &old_state, &mut dispatch);
        assert!(dispatch.messages.is_empty());
        assert_eq!(old_state.lock().unwrap().turn_countdown(), 7);

        let restarted = handler.handle_start(&mut new_room, 10);
        assert_response_code(&restarted, 10, Routes::START, WsResponseCode::OK);
        let current = handler.state("room").expect("new active hand");
        assert!(!Arc::ptr_eq(&old_state, &current));
        assert!(HoldemGameHandler::state_matches_room(
            &new_room, "room", &current
        ));
    }

    fn join_request(name: &str) -> ClientRequest {
        join_request_for_game(name, GameId::TEXAS_HOLD_EM)
    }

    fn join_with_hook(
        handler: &mut HoldemGameHandler,
        room: &mut RoomService,
        session_id: SessionId,
        name: &str,
    ) -> Dispatch {
        let request = join_request(name);
        let mut dispatch = room
            .handle_common_request(session_id, &request, handler.game_id(), || {
                handler.build_room_settings()
            })
            .expect("common JOIN dispatch");
        handler.after_common_request(room, session_id, &request, &mut dispatch);
        dispatch
    }

    fn join_response(dispatch: &Dispatch, session_id: SessionId) -> WsJoinResponse {
        dispatch
            .messages
            .iter()
            .find_map(|message| {
                if message.recipient != session_id {
                    return None;
                }
                let OutboundPayload::Response(RequestResponse::WithData(response)) =
                    &message.payload
                else {
                    return None;
                };
                (response.route == Routes::JOIN as i32
                    && response.code as i32 == WsResponseCode::JOINED as i32)
                    .then(|| serde_json::from_value(response.data.clone()).unwrap())
            })
            .expect("JOINED response")
    }

    fn table_snapshot(
        dispatch: &Dispatch,
        session_id: SessionId,
    ) -> WsTexasHoldEmTableSnapshotEvent {
        dispatch
            .messages
            .iter()
            .find_map(|message| {
                if message.recipient != session_id {
                    return None;
                }
                let OutboundPayload::Event(event) = &message.payload else {
                    return None;
                };
                (event.code == WsCode::TABLE_SNAPSHOT as i32)
                    .then(|| serde_json::from_value(event.data.clone()).unwrap())
            })
            .expect("table snapshot")
    }

    fn deal_for(dispatch: &Dispatch, session_id: SessionId) -> WsTexasHoldEmDealEvent {
        dispatch
            .messages
            .iter()
            .find_map(|message| {
                if message.recipient != session_id {
                    return None;
                }
                let OutboundPayload::Event(event) = &message.payload else {
                    return None;
                };
                (event.code == WsCode::DEAL as i32)
                    .then(|| serde_json::from_value(event.data.clone()).unwrap())
            })
            .expect("private deal")
    }

    fn assert_response_code(
        dispatch: &Dispatch,
        session_id: SessionId,
        route: Routes,
        code: WsResponseCode,
    ) {
        assert!(dispatch.messages.iter().any(|message| {
            message.recipient == session_id
                && match &message.payload {
                    OutboundPayload::Response(RequestResponse::WithoutData(response)) => {
                        response.route == route as i32 && response.code as i32 == code as i32
                    }
                    OutboundPayload::Response(RequestResponse::WithData(response)) => {
                        response.route == route as i32 && response.code as i32 == code as i32
                    }
                    OutboundPayload::Event(_) => false,
                }
        }));
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
