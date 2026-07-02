use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use share_type_public::{
    LandlordPhase, WsCode, WsPositionEvent,
    games::landlord::{
        WsCallLandlordEvent, WsDealEvent, WsDealOpenCardsEvent, WsLandlordGameOverEvent,
        WsPlayEvent, WsShowHiddenCardsEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{Delivery, OutboundPayload, RoomService, SessionSenders, dlog};

use crate::core::play::card_rank;
use crate::game_state::LandlordLoopState;
use crate::play_validator::validate_play_request;

enum AutoBroadcastEvent {
    Call(WsCallLandlordEvent),
    Play(WsPlayEvent),
}

fn build_auto_candidates(hand: &[i32]) -> Vec<Vec<i32>> {
    let grouped = group_by_rank(hand);
    let mut seen: HashSet<Vec<i32>> = HashSet::new();
    let mut candidates = Vec::new();

    // 顺子
    let single_ranks: Vec<u8> = grouped
        .iter()
        .filter_map(|(&rank, cards)| {
            if rank < 15 && !cards.is_empty() {
                Some(rank)
            } else {
                None
            }
        })
        .collect();
    let mut i = 0usize;
    while i < single_ranks.len() {
        let mut j = i;
        while j + 1 < single_ranks.len() && single_ranks[j + 1] == single_ranks[j] + 1 {
            j += 1;
        }
        let run = &single_ranks[i..=j];
        if run.len() >= 5 {
            for len in 5..=run.len() {
                for start in 0..=run.len() - len {
                    let mut cards = Vec::with_capacity(len);
                    for rank in &run[start..start + len] {
                        if let Some(&card) = grouped.get(rank).and_then(|v| v.first()) {
                            cards.push(card);
                        }
                    }
                    if cards.len() == len {
                        push_candidate(cards, &mut seen, &mut candidates);
                    }
                }
            }
        }
        i = j + 1;
    }

    // 3带（先三带一，再三带一对，再三张）
    let triple_ranks: Vec<u8> = grouped
        .iter()
        .filter_map(|(&rank, cards)| if cards.len() >= 3 { Some(rank) } else { None })
        .collect();
    for &triple_rank in &triple_ranks {
        let triple = grouped[&triple_rank][..3].to_vec();
        if let Some(single) = grouped
            .iter()
            .filter(|(rank, cards)| **rank != triple_rank && !cards.is_empty())
            .map(|(_, cards)| cards[0])
            .next()
        {
            let mut cards = triple.clone();
            cards.push(single);
            push_candidate(cards, &mut seen, &mut candidates);
        }
        if let Some(pair_cards) = grouped
            .iter()
            .filter(|(rank, cards)| **rank != triple_rank && cards.len() >= 2)
            .map(|(_, cards)| vec![cards[0], cards[1]])
            .next()
        {
            let mut cards = triple.clone();
            cards.extend(pair_cards);
            push_candidate(cards, &mut seen, &mut candidates);
        }
        push_candidate(triple, &mut seen, &mut candidates);
    }

    // 对子
    for cards in grouped.values().filter(|cards| cards.len() >= 2) {
        push_candidate(vec![cards[0], cards[1]], &mut seen, &mut candidates);
    }

    // 单张
    for cards in grouped.values() {
        if let Some(&card) = cards.first() {
            push_candidate(vec![card], &mut seen, &mut candidates);
        }
    }

    // 炸弹
    for cards in grouped.values().filter(|cards| cards.len() == 4) {
        push_candidate(cards.clone(), &mut seen, &mut candidates);
    }

    // 火箭
    if hand.contains(&53) && hand.contains(&54) {
        push_candidate(vec![53, 54], &mut seen, &mut candidates);
    }

    candidates
}

fn choose_auto_play(state: &LandlordLoopState, position: usize) -> Vec<i32> {
    let hand = match state.hands.get(&position) {
        Some(cards) => cards,
        None => return Vec::new(),
    };
    //怎么可能有空手牌？
    // if hand.is_empty() {
    //     return Vec::new();
    // }

    for candidate in build_auto_candidates(hand) {
        if validate_play_request(state, position, &candidate) {
            return candidate;
        }
    }

    // 兜底：轮到自己起牌时，至少出最小单张。
    if state.last_play.is_empty() || state.last_play_position == position {
        let mut smallest = hand.clone();
        smallest.sort_by_key(|card| (card_rank(*card), *card));
        return vec![smallest[0]];
    }
    Vec::new()
}

/// Build a Dispatch that sends an event to all room members.
fn dispatch_all(
    room_key: &str,
    code: i32,
    data: serde_json::Value,
    room_service: &RoomService,
) -> Vec<Delivery> {
    room_service
        .connected_session_ids(room_key)
        .into_iter()
        .map(|sid| Delivery {
            recipient: sid,
            payload: OutboundPayload::Event(share_type_public::CommonEvent {
                code,
                data: data.clone(),
            }),
        })
        .collect()
}

fn dispatch_phase(
    room_key: &str,
    state: &LandlordLoopState,
    room_service: &RoomService,
) -> Vec<Delivery> {
    dispatch_all(
        room_key,
        WsCode::CHANGE_PHASE as i32,
        serde_json::json!({
            "phase": state.phase as i32,
            "position": state.current_position as i32,
        }),
        room_service,
    )
}

/// Build a Dispatch that sends an event to a specific position in the room.
fn dispatch_to_position(
    room_key: &str,
    position: usize,
    code: i32,
    data: serde_json::Value,
    room_service: &RoomService,
) -> Vec<Delivery> {
    room_service
        .connected_session_ids_for_position(room_key, position)
        .into_iter()
        .map(|sid| Delivery {
            recipient: sid,
            payload: OutboundPayload::Event(share_type_public::CommonEvent {
                code,
                data: data.clone(),
            }),
        })
        .collect()
}

fn fixed_wait_seconds(
    configs: &std::collections::HashMap<String, i32>,
    key: &str,
    default: u64,
) -> u64 {
    configs.get(key).copied().unwrap_or(default as i32).max(0) as u64
}

fn group_by_rank(hand: &[i32]) -> BTreeMap<u8, Vec<i32>> {
    let mut grouped: BTreeMap<u8, Vec<i32>> = BTreeMap::new();
    for &card in hand {
        grouped.entry(card_rank(card)).or_default().push(card);
    }
    for cards in grouped.values_mut() {
        cards.sort_unstable();
    }
    grouped
}

/// Advance through the CallLandlord phase: move to next player, check completion.
///
/// When a full round completes (we're back at call_position), determine landlord:
/// - If nobody called (score == 0) → redeal
/// - Otherwise, the highest bidder (last in call_history with max score) becomes landlord
async fn handle_call_landlord_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    configs: &std::collections::HashMap<String, i32>,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    if loop_should_stop(state) {
        return;
    }
    let (open_hidden_event, next_turn_position, phase_changed): (
        Option<(String, Vec<i32>)>,
        Option<usize>,
        bool,
    ) = {
        let mut s = state.lock().unwrap();
        let current_idx = sorted_positions
            .iter()
            .position(|&p| p == s.current_position)
            .unwrap_or(0);
        let next_pos = sorted_positions[(current_idx + 1) % sorted_positions.len()];
        let should_finalize = if s.score == 3 {
            true
        } else {
            s.current_position = next_pos;
            s.current_position == s.call_position
        };

        if should_finalize {
            if s.score == 0 {
                // Everyone passed → redeal
                s.redeal();
                (None, Some(s.current_position), true)
            } else {
                // Find the player with the highest bid.
                // call_history is in call order; the highest score wins.
                // If tie (can't happen in 3-player since each round is one direction),
                // the last highest bidder wins.
                let max_score = s.call_history.iter().map(|(_, sc)| *sc).max().unwrap_or(0);
                let landlord_pos = s
                    .call_history
                    .iter()
                    .rev()
                    .find(|(_, sc)| *sc == max_score)
                    .map(|(pos, _)| *pos)
                    .unwrap_or(s.call_position);

                s.landlord_position = Some(landlord_pos);
                s.next_phase(); // CallLandlord → Play
                // Give hidden cards to landlord
                let hidden = s.hidden_cards.clone();
                if let Some(hand) = s.hands.get_mut(&landlord_pos) {
                    hand.extend(hidden.clone());
                    hand.sort_unstable();
                }
                // Reset for landlord's first play
                s.set_action_received(false);
                s.set_turn_countdown(turn_timeout(&s, configs));
                (
                    Some((player_name(&s, landlord_pos), hidden)),
                    Some(s.current_position),
                    true,
                )
            }
        } else {
            s.current_position = next_pos;
            // Reset for next caller in the same round
            s.set_action_received(false);
            s.set_turn_countdown(turn_timeout(&s, configs));
            (None, Some(s.current_position), false)
        }
    };

    if open_hidden_event.is_none() && next_turn_position.is_none() && !phase_changed {
        return;
    }

    let rs = room_service.lock().await;
    let mut dispatch = Vec::new();
    if phase_changed {
        let s = state.lock().unwrap();
        dispatch.extend(dispatch_phase(room_key, &s, &rs));
    }
    if let Some((name, cards)) = open_hidden_event {
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::DEAL_OPEN_CARDS as i32,
            serde_json::to_value(WsDealOpenCardsEvent { name, cards }).unwrap_or_default(),
            &rs,
        ));
    }
    if let Some(position) = next_turn_position {
        let countdown = state.lock().unwrap().turn_countdown();
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::CHANGE_DEAL as i32,
            serde_json::json!({
                "position": position as i32,
                "turn_countdown": countdown as i32,
            }),
            &rs,
        ));
    }
    drop(rs);
    send_dispatch(dispatch, senders).await;
}

/// Process a play tick: apply the current play (pass or cards), advance turns.
///
/// Logic:
/// - If current player played cards → update last_play, remove from hand, check win
/// - If current player passed (empty) → just advance
/// - When we loop back to last_play_position and the round leader didn't just pass,
///   the round leader gets to lead again (last_play cleared).
///
/// Returns true if the game is over (settlement).
async fn handle_play_phase(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    configs: &std::collections::HashMap<String, i32>,
    room_key: &str,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) -> bool {
    if loop_should_stop(state) {
        return true;
    }
    let mut game_over = false;
    let mut winner_pos = None;
    let mut next_turn_position: Option<usize> = None;

    {
        let mut s = state.lock().unwrap();
        let pos = s.current_position;
        let played = std::mem::take(&mut s.current_play);

        if played.is_empty() {
            // Player passed — nothing changes except who's next.
        } else {
            // Player played cards — this becomes the new benchmark
            s.last_play_position = pos;
            s.last_play = played;

            // Remove played cards from hand
            let play_back = s.last_play.clone();
            if let Some(hand) = s.hands.get_mut(&pos) {
                for card in &play_back {
                    if let Some(idx) = hand.iter().position(|c| c == card) {
                        hand.remove(idx);
                    }
                }
                // Check win condition
                if hand.is_empty() {
                    winner_pos = Some(pos);
                    s.next_phase(); // Play → Settlement
                    game_over = true;
                }
            }
        }

        if !game_over {
            // Advance to next position
            let current_idx = sorted_positions.iter().position(|&p| p == pos).unwrap_or(0);
            let next_pos = sorted_positions[(current_idx + 1) % sorted_positions.len()];
            s.current_position = next_pos;

            // If we've come full circle back to the last_play_position,
            // that means everyone else passed → last_play_position leads a new round
            if s.last_play_position == next_pos {
                s.last_play.clear();
            }

            // Reset for next turn
            s.set_action_received(false);
            s.set_turn_countdown(turn_timeout(&s, configs));
            next_turn_position = Some(next_pos);
        }
    }

    if let Some(position) = next_turn_position {
        let rs = room_service.lock().await;
        let countdown = state.lock().unwrap().turn_countdown();
        let dispatch = dispatch_all(
            room_key,
            WsCode::CHANGE_DEAL as i32,
            serde_json::json!({
                "position": position as i32,
                "turn_countdown": countdown as i32,
            }),
            &rs,
        );
        drop(rs);
        send_dispatch(dispatch, senders).await;
    }

    if game_over {
        let rs = room_service.lock().await;
        let mut dispatch = Vec::new();
        {
            let s = state.lock().unwrap();
            dispatch.extend(dispatch_phase(room_key, &s, &rs));
        }
        let (hidden_owner_name, hidden_cards, remaining_hands) = {
            let s = state.lock().unwrap();
            let hidden_cards = s.hidden_cards.clone();
            let players = s.players_snapshot();
            let hidden_owner_name = s
                .landlord_position
                .and_then(|pos| players.get(&pos).map(|(_, name)| name.clone()))
                .unwrap_or_default();
            let mut remaining_hands: Vec<(usize, Vec<i32>)> = s
                .hands
                .iter()
                .map(|(pos, cards)| (*pos, cards.clone()))
                .collect();
            remaining_hands.sort_by_key(|(pos, _)| *pos);
            let payloads = remaining_hands
                .into_iter()
                .map(|(pos, cards)| WsDealOpenCardsEvent {
                    name: players
                        .get(&pos)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_default(),
                    cards,
                })
                .collect::<Vec<_>>();
            (hidden_owner_name, hidden_cards, payloads)
        };

        // Broadcast hidden cards reveal
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::SHOW_HIDDEN_CARDS as i32,
            serde_json::to_value(WsShowHiddenCardsEvent {
                name: hidden_owner_name,
                cards: hidden_cards,
            })
            .unwrap_or_default(),
            &rs,
        ));
        // 显示每个人剩余的手牌
        for item in remaining_hands {
            dispatch.extend(dispatch_all(
                room_key,
                WsCode::DEAL_OPEN_CARDS as i32,
                serde_json::to_value(item).unwrap_or_default(),
                &rs,
            ));
        }

        // Broadcast game over
        let (is_landlord_win, landlord_position, score) = {
            let mut s = state.lock().unwrap();
            let is_win = s
                .landlord_position
                .map(|lp| winner_pos == Some(lp))
                .unwrap_or(false);
            let landlord_position = s.landlord_position;
            let score = s.score;
            s.apply_settlement_scores(is_win);
            (is_win, landlord_position, score)
        };
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::GAME_OVER as i32,
            serde_json::to_value(WsLandlordGameOverEvent {
                is_landlord: is_landlord_win,
            })
            .unwrap_or_default(),
            &rs,
        ));
        drop(rs);
        crate::official::settle_round(
            room_service,
            room_key,
            landlord_position,
            is_landlord_win,
            score,
        )
        .await;
        send_dispatch(dispatch, senders).await;
        return true;
    }

    false
}

async fn handle_settlement_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    configs: &std::collections::HashMap<String, i32>,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) -> bool {
    // 人数仍是 3 且没有断线玩家，才等待结算后进入下一局；否则结束循环。
    if settlement_should_stop(state) {
        return true;
    }
    if sleep_or_stop(
        state,
        Duration::from_secs(fixed_wait_seconds(configs, "settlement_time", 5)),
    )
    .await
    {
        return true;
    }
    let (phase, position) = {
        let mut s = state.lock().unwrap();
        if s.phase != LandlordPhase::Settlement {
            return false;
        }
        if s.stop_requested() || s.players_snapshot().len() != 3 || s.has_disconnected_players() {
            return true;
        }
        s.redeal();
        (s.phase as i32, s.current_position as i32)
    };
    let rs = room_service.lock().await;
    let dispatch = dispatch_all(
        room_key,
        WsCode::CHANGE_PHASE as i32,
        serde_json::json!({
            "phase": phase,
            "position": position,
        }),
        &rs,
    );
    drop(rs);
    send_dispatch(dispatch, senders).await;
    false
}

// ─── Phase Handlers ───────────────────────────────────────────────────

/// Start → deal cards, wait fixed start_time, then advance to CallLandlord.
async fn handle_start_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    configs: &std::collections::HashMap<String, i32>,
) -> bool {
    // 1. Deal cards inside lock
    let deal_data: Vec<(usize, Vec<i32>, String)>;

    {
        let mut s = state.lock().unwrap();
        if s.stop_requested() || s.players_snapshot().len() != sorted_positions.len() {
            return true;
        }
        if s.phase != LandlordPhase::Start {
            return false;
        }
        s.generate_card();
        deal_data = sorted_positions
            .iter()
            .filter_map(|&pos| {
                let hand = s.hands.get(&pos)?.clone();
                Some((pos, hand, player_name(&s, pos)))
            })
            .collect();
    };

    // 2. Send events (outside lock)
    let rs = room_service.lock().await;
    let mut dispatch = Vec::new();
    for (pos, hand, name) in &deal_data {
        dispatch.extend(dispatch_to_position(
            room_key,
            *pos,
            WsCode::DEAL as i32,
            serde_json::to_value(WsDealEvent {
                name: name.clone(),
                cards: hand.clone(),
            })
            .unwrap_or_default(),
            &rs,
        ));
    }
    drop(rs);
    send_dispatch(dispatch, senders).await;

    if sleep_or_stop(
        state,
        Duration::from_secs(fixed_wait_seconds(configs, "start_time", 1)),
    )
    .await
    {
        return true;
    }

    let (first_call_position, phase_payload) = {
        let mut s = state.lock().unwrap();
        if s.stop_requested() || s.players_snapshot().len() != sorted_positions.len() {
            return true;
        }
        if s.phase != LandlordPhase::Start {
            return false;
        }
        s.next_phase(); // Start → CallLandlord
        s.set_action_received(false);
        s.set_turn_countdown(turn_timeout(&s, configs));
        (
            s.current_position,
            (s.phase as i32, s.current_position as i32),
        )
    };
    let rs = room_service.lock().await;
    let mut dispatch = dispatch_all(
        room_key,
        WsCode::CHANGE_PHASE as i32,
        serde_json::json!({
            "phase": phase_payload.0,
            "position": phase_payload.1,
        }),
        &rs,
    );
    dispatch.extend(dispatch_all(
        room_key,
        WsCode::CHANGE_DEAL as i32,
        serde_json::json!({
            "position": first_call_position as i32,
            "turn_countdown": state.lock().unwrap().turn_countdown() as i32,
        }),
        &rs,
    ));
    drop(rs);
    send_dispatch(dispatch, senders).await;
    false
}

/// Handle timeout: mark the current player as away and simulate their action.
fn handle_timeout(state: &mut LandlordLoopState) -> (Option<usize>, Option<AutoBroadcastEvent>) {
    let pos = state.current_position;
    let newly_away = if state.mark_away(pos) {
        Some(pos)
    } else {
        None
    };
    let mut auto_event = None;
    // 广播away

    match state.phase {
        LandlordPhase::CallLandlord => {
            // Timed out = no call (score 0). Record it.
            let name = state.player_name(pos);
            state.call_history.push((pos, 0));
            println!(
                "[landlord][auto-call] pos={} name={} timeout -> score=0 history_len={}",
                pos,
                name,
                state.call_history.len()
            );
            auto_event = Some(AutoBroadcastEvent::Call(WsCallLandlordEvent {
                name,
                score: 0,
            }));
            next_call(state);
        }
        LandlordPhase::Play => {
            // Timed out = pass (empty play)
            // 自动出牌，能管上就管，如果自己最大率先出牌，必出最小的牌，先看能不能顺，再看能不能3带，再看对，最后出单。
            let auto_cards = choose_auto_play(state, pos);
            let name = state.player_name(pos);
            println!(
                "[landlord][auto-play] pos={} name={} timeout -> cards={:?}",
                pos, name, auto_cards
            );
            auto_event = Some(AutoBroadcastEvent::Play(WsPlayEvent {
                name,
                cards: auto_cards.clone(),
            }));
            state.current_play = auto_cards;
            next_play(state);
        }
        _ => {}
    }
    (newly_away, auto_event)
}

fn loop_should_stop(state: &Arc<std::sync::Mutex<LandlordLoopState>>) -> bool {
    let s = state.lock().unwrap();
    s.stop_requested() || s.players_snapshot().len() != 3
}

fn next_call(state: &mut LandlordLoopState) {
    state.set_action_received(true);
}

fn next_play(state: &mut LandlordLoopState) {
    state.set_action_received(true);
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn player_name(state: &LandlordLoopState, position: usize) -> String {
    state.player_name(position)
}

fn push_candidate(cards: Vec<i32>, seen: &mut HashSet<Vec<i32>>, out: &mut Vec<Vec<i32>>) {
    let mut normalized = cards;
    normalized.sort_unstable();
    if seen.insert(normalized.clone()) {
        out.push(normalized);
    }
}

/// Send a dispatch to all recipients via session senders.
async fn send_dispatch(dispatch: Vec<Delivery>, senders: &SessionSenders) {
    let mut encoded = Vec::with_capacity(dispatch.len());
    for delivery in dispatch {
        if let Ok(frame) = ws_common::to_text_message(&delivery.payload) {
            encoded.push((delivery.recipient, frame));
        }
    }
    let senders = senders.lock().await;
    for (recipient, frame) in encoded {
        if let Some(tx) = senders.get(&recipient) {
            let _ = tx.send(frame);
        }
    }
}

fn settlement_should_stop(state: &Arc<std::sync::Mutex<LandlordLoopState>>) -> bool {
    let s = state.lock().unwrap();
    s.stop_requested() || s.players_snapshot().len() != 3 || s.has_disconnected_players()
}

async fn sleep_or_stop(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    duration: Duration,
) -> bool {
    let mut remaining = duration.as_millis();
    while remaining > 0 {
        if loop_should_stop(state) {
            return true;
        }
        let step = remaining.min(100) as u64;
        tokio::time::sleep(Duration::from_millis(step)).await;
        remaining -= u128::from(step);
    }
    loop_should_stop(state)
}

// ─── Game Loop ────────────────────────────────────────────────────────

pub(crate) fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<LandlordLoopState>>,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    loop_states: Arc<std::sync::Mutex<HashMap<String, Arc<std::sync::Mutex<LandlordLoopState>>>>>,
) {
    tokio::spawn(async move {
        let sorted_positions: Vec<usize> = {
            let s = state.lock().unwrap();
            let mut p: Vec<usize> = s.players_snapshot().keys().copied().collect();
            p.sort();
            p
        };

        // Read configs once; they won't change mid-game.
        let configs: std::collections::HashMap<String, i32> = {
            room_service
                .lock()
                .await
                .get_room_configs(&room_key)
                .unwrap_or_default()
        };

        loop {
            if state.lock().unwrap().stop_requested() {
                break;
            }
            let paused = { state.lock().unwrap().is_paused() };
            if paused {
                if sleep_or_stop(&state, Duration::from_secs(1)).await {
                    break;
                }
                continue;
            }

            let current_phase = { state.lock().unwrap().phase };
            let player_num = state.lock().unwrap().base.lock().unwrap().players.len();
            dlog!(
                ws_common::tracing::Level::INFO,
                "[landlord][game-loop] room={} phase={:?} play number ={} action_received={}",
                room_key,
                current_phase,
                player_num,
                state.lock().unwrap().action_received()
            );
            if player_num != 3 {
                break;
            }

            match current_phase {
                LandlordPhase::Start => {
                    let should_stop = handle_start_phase(
                        &room_key,
                        &state,
                        &sorted_positions,
                        &room_service,
                        &senders,
                        &configs,
                    )
                    .await;
                    if should_stop {
                        break;
                    }
                }
                LandlordPhase::CallLandlord | LandlordPhase::Play => {
                    let phase = { state.lock().unwrap().phase };
                    let mut away_position: Option<usize> = None;
                    let mut auto_event: Option<AutoBroadcastEvent> = None;
                    if matches!(phase, LandlordPhase::CallLandlord | LandlordPhase::Play) {
                        let should_wait_tick = {
                            let s = state.lock().unwrap();
                            !s.action_received()
                        };
                        if should_wait_tick {
                            if sleep_or_stop(&state, Duration::from_secs(1)).await {
                                break;
                            }
                            if state.lock().unwrap().is_paused() {
                                continue;
                            }
                            let mut s = state.lock().unwrap();
                            if !matches!(s.phase, LandlordPhase::CallLandlord | LandlordPhase::Play)
                            {
                                continue;
                            }
                            if s.action_received() {
                                // Action received while waiting this tick.
                            } else if s.turn_countdown() > 0 {
                                s.set_turn_countdown(s.turn_countdown() - 1);
                                continue; // Wait for action or timeout
                            } else {
                                // Timeout
                                (away_position, auto_event) = handle_timeout(&mut s);
                            }
                        }
                    }
                    if away_position.is_some() || auto_event.is_some() {
                        let rs = room_service.lock().await;
                        let mut dispatch = Vec::new();
                        if let Some(position) = away_position {
                            dispatch.extend(dispatch_all(
                                &room_key,
                                WsCode::AWAY as i32,
                                serde_json::to_value(WsPositionEvent {
                                    position: position as i32,
                                })
                                .unwrap_or_default(),
                                &rs,
                            ));
                        }
                        if let Some(event) = auto_event {
                            match event {
                                AutoBroadcastEvent::Call(payload) => {
                                    dispatch.extend(dispatch_all(
                                        &room_key,
                                        WsCode::CALL_LANDLORD as i32,
                                        serde_json::to_value(payload).unwrap_or_default(),
                                        &rs,
                                    ));
                                }
                                AutoBroadcastEvent::Play(payload) => {
                                    dispatch.extend(dispatch_all(
                                        &room_key,
                                        WsCode::PLAY as i32,
                                        serde_json::to_value(payload).unwrap_or_default(),
                                        &rs,
                                    ));
                                }
                            }
                        }
                        drop(rs);
                        send_dispatch(dispatch, &senders).await;
                    }

                    if loop_should_stop(&state) {
                        break;
                    }

                    let phase_after_tick = { state.lock().unwrap().phase };
                    match phase_after_tick {
                        LandlordPhase::CallLandlord => {
                            handle_call_landlord_phase(
                                &room_key,
                                &state,
                                &sorted_positions,
                                &configs,
                                &room_service,
                                &senders,
                            )
                            .await;
                        }
                        LandlordPhase::Play => {
                            let _ended = handle_play_phase(
                                &state,
                                &sorted_positions,
                                &configs,
                                &room_key,
                                &room_service,
                                &senders,
                            )
                            .await;
                        }
                        _ => {}
                    }
                }
                LandlordPhase::Settlement => {
                    if handle_settlement_phase(&room_key, &state, &configs, &room_service, &senders)
                        .await
                    {
                        break;
                    }
                }
            }
        }

        // Cleanup
        let common = { Arc::clone(&state.lock().unwrap().base) };
        room_service
            .lock()
            .await
            .clear_room_game_state_if_same(&room_key, &common);
        let mut states = loop_states.lock().unwrap();
        if states
            .get(&room_key)
            .is_some_and(|current| Arc::ptr_eq(current, &state))
        {
            states.remove(&room_key);
        }
    });
}

/// Get the per-turn timeout for the current position:
/// away players use away_time, others use play_time.
fn turn_timeout(
    state: &LandlordLoopState,
    configs: &std::collections::HashMap<String, i32>,
) -> u32 {
    let away_time = configs.get("away_time").copied().unwrap_or(5) as u32;
    let play_time = configs.get("play_time").copied().unwrap_or(30) as u32;
    if state.is_away(state.current_position) {
        away_time
    } else {
        play_time
    }
}
