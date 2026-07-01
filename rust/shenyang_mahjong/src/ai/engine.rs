use std::collections::HashMap;

use ws_common::{Dispatch, RoomService};

use crate::game::{perform_discard, perform_self_draw_hu, resolve_claim_window};
use crate::game_state::{ClaimResponse, ShenyangMahjongLoopState};
use crate::rules::is_standard_win;

use super::decision::{AiClaimChoice, choose_claim_from_view, choose_discard_from_view};
use super::observation::build_public_table;

fn self_hand(state: &ShenyangMahjongLoopState, position: usize) -> Option<Vec<i32>> {
    state.hands.get(&position).cloned()
}

pub fn maybe_play_ai_turn(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) -> bool {
    if state.phase != share_type_public::games::shenyang_mahjong::ShenyangMahjongPhase::Play
        || state.claim_window.is_some()
    {
        return false;
    }
    let position = state.current_position;
    if !state.is_ai_position(position) {
        return false;
    }

    let Some(hand) = self_hand(state, position) else {
        return false;
    };
    if hand.is_empty() {
        return false;
    }
    if is_standard_win(&hand) {
        perform_self_draw_hu(room_service, room_key, state, dispatch, position);
        return true;
    }

    let table = build_public_table(state);
    if let Some(tile) = choose_discard_from_view(&hand, &table, position) {
        return perform_discard(room_service, room_key, state, configs, dispatch, position, tile);
    }
    false
}

pub fn maybe_resolve_ai_claims(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) -> bool {
    let Some(claim_window) = state.claim_window.clone() else {
        return false;
    };
    let table = build_public_table(state);
    let Some(claim) = table.claim_window.as_ref() else {
        return false;
    };

    let mut changed = false;
    for position in claim_window.eligible_positions {
        if claim_window.responses.contains_key(&position) || !state.is_ai_position(position) {
            continue;
        }
        let Some(hand) = self_hand(state, position) else {
            continue;
        };
        let choice =
            choose_claim_from_view(&hand, claim, &table, position).unwrap_or(AiClaimChoice::Pass);
        let response = match choice {
            AiClaimChoice::Hu => ClaimResponse::Hu,
            AiClaimChoice::Peng => ClaimResponse::Peng,
            AiClaimChoice::Chi { consume_tiles } => ClaimResponse::Chi { consume_tiles },
            AiClaimChoice::Pass => ClaimResponse::Pass,
        };
        if let Some(current) = state.claim_window.as_mut() {
            current.responses.insert(position, response);
            changed = true;
        }
    }

    if changed {
        let all_received = state
            .claim_window
            .as_ref()
            .map(|window| {
                window
                    .eligible_positions
                    .iter()
                    .all(|item| window.responses.contains_key(item))
            })
            .unwrap_or(false);
        if all_received {
            resolve_claim_window(room_service, room_key, state, configs, dispatch);
            return true;
        }
    }
    false
}

