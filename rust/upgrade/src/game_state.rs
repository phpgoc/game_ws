use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rand::seq::SliceRandom;
use share_type_public::{
    UpgradePhase, UpgradeRank, WsUpgradePlayedCards, WsUpgradeTableSnapshotEvent,
};
use ws_common::game_state::{CommonGameState, GameState};

#[derive(Debug, Clone)]
pub struct UpgradeRules {
    pub blood_enabled: bool,
    pub blood_score_per_unit: i32,
    pub blood_start_score: i32,
    pub bottom_card_count: usize,
    pub deck_count: usize,
    pub target_rank: UpgradeRank,
}

#[derive(Debug)]
pub struct UpgradeGameState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: UpgradePhase,
    pub rules: UpgradeRules,
    pub hands: HashMap<usize, Vec<i32>>,
    pub bottom_cards: Vec<i32>,
    pub dealer_position: usize,
    pub current_position: usize,
    pub trick_index: i32,
    pub current_trick: Vec<WsUpgradePlayedCards>,
}

pub type UpgradeStateHandle = Arc<Mutex<UpgradeGameState>>;

impl UpgradeRules {
    pub fn blood_units(&self, score: i32) -> i32 {
        if !self.blood_enabled || score < self.blood_start_score {
            return 0;
        }
        ((score - self.blood_start_score) / self.blood_score_per_unit.max(1)) + 1
    }
}

impl UpgradeGameState {
    pub fn from_common(base: Arc<Mutex<CommonGameState>>) -> Self {
        Self {
            base,
            phase: UpgradePhase::Start,
            rules: UpgradeRules {
                blood_enabled: true,
                blood_score_per_unit: 40,
                blood_start_score: 80,
                bottom_card_count: 8,
                deck_count: 2,
                target_rank: UpgradeRank::A,
            },
            hands: HashMap::new(),
            bottom_cards: Vec::new(),
            dealer_position: 0,
            current_position: 0,
            trick_index: 0,
            current_trick: Vec::new(),
        }
    }

    pub fn active_positions(&self) -> Vec<usize> {
        let mut positions: Vec<_> = self.base.lock().unwrap().players.keys().copied().collect();
        positions.sort_unstable();
        positions
    }

    pub fn deal_new_round(&mut self, mut rules: UpgradeRules) -> Result<(), &'static str> {
        rules.deck_count = rules.deck_count.clamp(2, 4);
        rules.blood_score_per_unit = rules.blood_score_per_unit.max(1);
        let positions = self.active_positions();
        if positions.len() != 4 {
            return Err("Upgrade requires exactly 4 players");
        }

        let mut deck = build_upgrade_deck(rules.deck_count);
        deck.shuffle(&mut rand::rng());
        rules.bottom_card_count = adjusted_bottom_card_count(
            deck.len(),
            positions.len(),
            rules.bottom_card_count,
            min_bottom_card_count(rules.deck_count),
        )
        .ok_or("invalid bottom card count")?;

        self.phase = UpgradePhase::Deal;
        self.rules = rules;
        self.hands.clear();
        self.bottom_cards.clear();
        self.current_trick.clear();
        self.trick_index = 0;
        self.dealer_position = positions[0];
        self.current_position = self.dealer_position;

        for _ in 0..self.rules.bottom_card_count {
            if let Some(card) = deck.pop() {
                self.bottom_cards.push(card);
            }
        }
        for (idx, card) in deck.into_iter().enumerate() {
            let position = positions[idx % positions.len()];
            self.hands.entry(position).or_default().push(card);
        }
        for hand in self.hands.values_mut() {
            hand.sort_unstable();
        }
        self.phase = UpgradePhase::Play;
        self.base.lock().unwrap().action_received = false;
        Ok(())
    }

    pub fn hand_count(&self) -> usize {
        self.hands.values().next().map(Vec::len).unwrap_or_default()
    }

    pub fn is_finished(&self) -> bool {
        !self.hands.is_empty() && self.hands.values().all(Vec::is_empty)
    }

    pub fn next_position(&self, from: usize) -> Option<usize> {
        let positions = self.active_positions();
        let start = positions.iter().position(|position| *position == from)?;
        Some(positions[(start + 1) % positions.len()])
    }

    pub fn play_cards(
        &mut self,
        position: usize,
        name: String,
        cards: Vec<i32>,
    ) -> Result<WsUpgradePlayedCards, &'static str> {
        if self.phase != UpgradePhase::Play || self.current_position != position || cards.is_empty()
        {
            return Err("not current turn");
        }
        remove_cards_from_hand(self.hands.entry(position).or_default(), &cards)?;
        let played = WsUpgradePlayedCards {
            position: position as i32,
            name,
            cards,
        };
        self.current_trick.push(played.clone());
        if self.current_trick.len() >= self.active_positions().len() {
            self.current_trick.clear();
            self.trick_index += 1;
        }
        self.current_position = self.next_position(position).unwrap_or(position);
        if self.is_finished() {
            self.phase = UpgradePhase::Settlement;
        }
        self.base.lock().unwrap().action_received = true;
        Ok(played)
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn remaining_hand_count(&self, position: usize) -> i32 {
        self.hands
            .get(&position)
            .map(|cards| cards.len() as i32)
            .unwrap_or_default()
    }

    pub fn set_turn_countdown(&mut self, countdown: u32) {
        self.base.lock().unwrap().turn_countdown = countdown;
    }

    pub fn snapshot(&self) -> WsUpgradeTableSnapshotEvent {
        WsUpgradeTableSnapshotEvent {
            phase: self.phase,
            deck_count: self.rules.deck_count as i32,
            target_rank: self.rules.target_rank,
            blood_enabled: self.rules.blood_enabled,
            blood_start_score: self.rules.blood_start_score,
            blood_score_per_unit: self.rules.blood_score_per_unit,
            bottom_card_count: self.bottom_cards.len() as i32,
            hand_count: self.hand_count() as i32,
            dealer_position: self.dealer_position as i32,
            current_position: self.current_position as i32,
            trick_index: self.trick_index,
            current_trick: self.current_trick.clone(),
            turn_countdown: self.base.lock().unwrap().turn_countdown as i32,
        }
    }
}

impl GameState for UpgradeGameState {
    fn can_accept_players(&self) -> bool {
        self.phase == UpgradePhase::Start
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.base)
    }
}

pub fn adjusted_bottom_card_count(
    total_cards: usize,
    player_count: usize,
    preferred: usize,
    minimum: usize,
) -> Option<usize> {
    if player_count == 0 || minimum >= total_cards {
        return None;
    }
    let max_bottom = total_cards.saturating_sub(player_count);
    let preferred = preferred.max(minimum).min(max_bottom);
    (preferred..=max_bottom)
        .find(|bottom| (total_cards - bottom) % player_count == 0)
        .or_else(|| {
            (minimum..preferred)
                .rev()
                .find(|bottom| (total_cards - bottom) % player_count == 0)
        })
}

pub fn min_bottom_card_count(deck_count: usize) -> usize {
    match deck_count {
        3 => 10,
        2 | 4 => 8,
        _ => 8,
    }
}

pub fn build_upgrade_deck(deck_count: usize) -> Vec<i32> {
    let deck_count = deck_count.clamp(2, 4);
    let mut cards = Vec::with_capacity(deck_count * 54);
    for deck_index in 0..deck_count {
        let offset = deck_index as i32 * 100;
        for card in 1..=54 {
            cards.push(offset + card);
        }
    }
    cards
}

fn remove_cards_from_hand(hand: &mut Vec<i32>, cards: &[i32]) -> Result<(), &'static str> {
    let mut indexes = Vec::with_capacity(cards.len());
    for card in cards {
        let Some(idx) = hand
            .iter()
            .enumerate()
            .find_map(|(idx, current)| (!indexes.contains(&idx) && current == card).then_some(idx))
        else {
            return Err("card not in hand");
        };
        indexes.push(idx);
    }
    indexes.sort_unstable_by(|a, b| b.cmp(a));
    for idx in indexes {
        hand.remove(idx);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjusted_bottom_keeps_all_hands_equal() {
        for deck_count in 2..=4 {
            let total = build_upgrade_deck(deck_count).len();
            let bottom =
                adjusted_bottom_card_count(total, 4, 8, min_bottom_card_count(deck_count)).unwrap();
            assert_eq!((total - bottom) % 4, 0);
            assert!(bottom >= min_bottom_card_count(deck_count));
        }
    }

    #[test]
    fn blood_units_start_after_threshold() {
        let rules = UpgradeRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            target_rank: UpgradeRank::A,
        };
        assert_eq!(rules.blood_units(79), 0);
        assert_eq!(rules.blood_units(80), 1);
        assert_eq!(rules.blood_units(120), 2);
    }

    #[test]
    fn three_decks_uses_at_least_ten_bottom_cards() {
        let total = build_upgrade_deck(3).len();
        let bottom = adjusted_bottom_card_count(total, 4, 8, min_bottom_card_count(3)).unwrap();
        assert_eq!(bottom, 10);
        assert_eq!((total - bottom) % 4, 0);
    }
}
