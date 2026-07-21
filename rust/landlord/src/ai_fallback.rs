use share_type_public::LandlordPhase;

use crate::{
    core::play::{ComboKind, card_rank, classify},
    game_state::LandlordLoopState,
};

pub fn choose_bid(_state: &LandlordLoopState, _position: usize) -> u8 {
    0
}

pub fn choose_play(state: &LandlordLoopState, position: usize) -> Vec<i32> {
    if state.phase != LandlordPhase::Play || state.current_position != position {
        return Vec::new();
    }
    let leading = state.last_play.is_empty() || state.last_play_position == position;
    let hand = state.hands.get(&position).map(Vec::as_slice).unwrap_or(&[]);
    if leading {
        return hand.first().copied().into_iter().collect();
    }
    let Some(benchmark) = classify(&state.last_play) else {
        return Vec::new();
    };
    if benchmark.kind != ComboKind::Single {
        return Vec::new();
    }
    hand.iter()
        .copied()
        .filter(|card| card_rank(*card) > benchmark.main_rank)
        .min_by_key(|card| card_rank(*card))
        .into_iter()
        .collect()
}

pub fn hand_has_bomb(hand: &[i32]) -> bool {
    let mut counts = [0_u8; 18];
    for &card in hand {
        counts[card_rank(card) as usize] += 1;
    }
    counts[3..=15].contains(&4) || (counts[16] == 1 && counts[17] == 1)
}
