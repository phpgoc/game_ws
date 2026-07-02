use std::collections::HashMap;

use crate::game_state::ShenyangMahjongLoopState;

#[derive(Debug, Clone)]
pub struct AiSeatView {
    pub position: usize,
    pub is_ai: bool,
    pub is_away: bool,
    pub hand_count: usize,
    pub discards: Vec<i32>,
    pub melds: Vec<share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld>,
}

#[derive(Debug, Clone)]
pub struct AiPublicTable {
    pub current_position: usize,
    pub dealer_position: usize,
    pub wall_count: usize,
    pub claim_window: Option<AiClaimView>,
    pub seats: HashMap<usize, AiSeatView>,
}

#[derive(Debug, Clone)]
pub struct AiClaimView {
    pub tile: i32,
    pub from_position: usize,
    pub eligible_positions: Vec<usize>,
}

pub fn build_public_table(state: &ShenyangMahjongLoopState) -> AiPublicTable {
    let players = state.players_snapshot();
    let mut seats = HashMap::new();
    for (position, _) in players {
        seats.insert(
            position,
            AiSeatView {
                position,
                is_ai: state.is_ai_position(position),
                is_away: state.is_away(position),
                hand_count: state
                    .hands
                    .get(&position)
                    .map(|hand| hand.len())
                    .unwrap_or(0),
                discards: state.discards.get(&position).cloned().unwrap_or_default(),
                melds: state.melds.get(&position).cloned().unwrap_or_default(),
            },
        );
    }

    let claim_window = state.claim_window.as_ref().map(|window| AiClaimView {
        tile: window.tile,
        from_position: window.from_position,
        eligible_positions: window.eligible_positions.clone(),
    });

    AiPublicTable {
        current_position: state.current_position,
        dealer_position: state.dealer_position,
        wall_count: state.wall_count(),
        claim_window,
        seats,
    }
}
