use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rand::seq::SliceRandom;
use share_type_public::{
    TractorPhase, TractorRank, WsTractorPlayedCards, WsTractorTableSnapshotEvent,
};
use ws_common::game_state::{CommonGameState, GameState};

#[derive(Debug)]
pub struct TractorGameState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: TractorPhase,
    pub rules: TractorRules,
    pub hands: HashMap<usize, Vec<i32>>,
    pub bottom_cards: Vec<i32>,
    pub bottom_multiplier: i32,
    pub collected_scores: HashMap<usize, i32>,
    pub last_trick_winner: Option<usize>,
    pub dealer_position: usize,
    pub current_position: usize,
    pub round_index: i32,
    pub trick_index: i32,
    pub current_trick: Vec<WsTractorPlayedCards>,
}

#[derive(Debug, Clone)]
pub struct TractorRules {
    pub blood_enabled: bool,
    pub blood_score_per_unit: i32,
    pub blood_start_score: i32,
    pub bottom_card_count: usize,
    pub deck_count: usize,
    pub final_target_rank: TractorRank,
    pub removed_rank_mask: i32,
    pub target_rank: TractorRank,
}

pub type TractorStateHandle = Arc<Mutex<TractorGameState>>;

pub const TRACTOR_RANKS: [TractorRank; 13] = [
    TractorRank::TWO,
    TractorRank::THREE,
    TractorRank::FOUR,
    TractorRank::FIVE,
    TractorRank::SIX,
    TractorRank::SEVEN,
    TractorRank::EIGHT,
    TractorRank::NINE,
    TractorRank::TEN,
    TractorRank::J,
    TractorRank::Q,
    TractorRank::K,
    TractorRank::A,
];

pub fn tractor_rank_from_setting_index(index: i32) -> TractorRank {
    TRACTOR_RANKS
        .get(index.clamp(0, TRACTOR_RANKS.len() as i32 - 1) as usize)
        .copied()
        .unwrap_or(TractorRank::A)
}

pub fn tractor_rank_mask(ranks: &[TractorRank]) -> i32 {
    ranks.iter().fold(0, |mask, rank| {
        TRACTOR_RANKS
            .iter()
            .position(|item| item == rank)
            .map(|idx| mask | (1_i32 << idx))
            .unwrap_or(mask)
    })
}

fn rank_is_removed(mask: i32, rank: TractorRank) -> bool {
    TRACTOR_RANKS
        .iter()
        .position(|item| *item == rank)
        .map(|idx| mask & (1_i32 << idx) != 0)
        .unwrap_or(false)
}

pub fn tractor_rank_path(
    removed_rank_mask: i32,
    final_target_rank: TractorRank,
) -> Vec<TractorRank> {
    let mut out = Vec::new();
    for rank in TRACTOR_RANKS {
        if rank as i32 > final_target_rank as i32 {
            break;
        }
        if rank == final_target_rank || !rank_is_removed(removed_rank_mask, rank) {
            out.push(rank);
        }
    }
    if out.is_empty() {
        out.push(final_target_rank);
    }
    out
}

fn first_match_rank(removed_rank_mask: i32, final_target_rank: TractorRank) -> TractorRank {
    tractor_rank_path(removed_rank_mask, final_target_rank)
        .first()
        .copied()
        .unwrap_or(final_target_rank)
}

fn next_match_rank(
    current_rank: TractorRank,
    removed_rank_mask: i32,
    final_target_rank: TractorRank,
) -> Option<TractorRank> {
    let path = tractor_rank_path(removed_rank_mask, final_target_rank);
    let index = path.iter().position(|rank| *rank == current_rank)?;
    path.get(index + 1).copied()
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

fn base_card(card: i32) -> i32 {
    ((card - 1) % 100) + 1
}

pub fn build_tractor_deck(deck_count: usize) -> Vec<i32> {
    build_tractor_deck_with_removed_ranks(deck_count, 0)
}

pub fn build_tractor_deck_with_removed_ranks(
    deck_count: usize,
    removed_rank_mask: i32,
) -> Vec<i32> {
    let deck_count = deck_count.clamp(2, 4);
    let mut cards = Vec::with_capacity(deck_count * 54);
    for deck_index in 0..deck_count {
        let offset = deck_index as i32 * 100;
        for card in 1..=54 {
            let full_card = offset + card;
            let rank = card_rank(full_card);
            let should_remove = TRACTOR_RANKS
                .iter()
                .find(|item| **item as i32 == rank)
                .is_some_and(|rank| rank_is_removed(removed_rank_mask, *rank));
            if !should_remove {
                cards.push(full_card);
            }
        }
    }
    cards
}

fn card_matches_play_suit(card: i32, suit: Option<i32>, rules: &TractorRules) -> bool {
    match suit {
        Some(lead_suit) => {
            !is_trump_card(card, rules.target_rank) && card_suit(card) == Some(lead_suit)
        }
        None => is_trump_card(card, rules.target_rank),
    }
}

fn card_rank(card: i32) -> i32 {
    let base = base_card(card);
    if base <= 52 {
        ((base - 1) % 13) + 2
    } else if base == 53 {
        16
    } else {
        17
    }
}

fn card_score(card: i32) -> i32 {
    match card_rank(card) {
        5 => 5,
        10 | 13 => 10,
        _ => 0,
    }
}

fn card_suit(card: i32) -> Option<i32> {
    let base = base_card(card);
    (base <= 52).then_some((base - 1) / 13)
}

fn is_trump_card(card: i32, target_rank: TractorRank) -> bool {
    card_suit(card).is_none() || card_rank(card) == target_rank as i32
}

pub fn min_bottom_card_count(deck_count: usize) -> usize {
    match deck_count {
        3 => 10,
        2 | 4 => 8,
        _ => 8,
    }
}

fn must_follow_play_suit(
    hand: &[i32],
    suit: Option<i32>,
    count: usize,
    rules: &TractorRules,
) -> bool {
    hand.iter()
        .filter(|card| card_matches_play_suit(**card, suit, rules))
        .count()
        >= count
}

fn play_rank(cards: &[i32]) -> Option<i32> {
    let first = cards.first().copied().map(card_rank)?;
    cards
        .iter()
        .all(|card| card_rank(*card) == first)
        .then_some(first)
}

fn play_suit(cards: &[i32], rules: &TractorRules) -> Option<i32> {
    if cards
        .iter()
        .any(|card| is_trump_card(*card, rules.target_rank))
    {
        None
    } else {
        cards.first().and_then(|card| card_suit(*card))
    }
}

fn played_score(cards: &[i32]) -> i32 {
    cards.iter().map(|card| card_score(*card)).sum()
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

fn same_play_shape(cards: &[i32]) -> bool {
    !cards.is_empty() && play_rank(cards).is_some()
}

fn team_positions(position: usize) -> [usize; 2] {
    [position, (position + 2) % 4]
}

fn tractor_card_value(card: i32, rules: &TractorRules, lead_suit: Option<i32>) -> i32 {
    let rank = card_rank(card);
    if is_trump_card(card, rules.target_rank) {
        return match card_suit(card) {
            None => 1_000 + rank,
            Some(_) => 900 + rank,
        };
    }
    if card_suit(card) == lead_suit {
        return 500 + rank;
    }
    rank
}

fn trick_winner(trick: &[WsTractorPlayedCards], rules: &TractorRules) -> Option<usize> {
    let lead = trick.first()?;
    let lead_suit = play_suit(&lead.cards, rules);
    trick
        .iter()
        .filter_map(|played| {
            let position = usize::try_from(played.position).ok()?;
            let value = played
                .cards
                .iter()
                .map(|card| tractor_card_value(*card, rules, lead_suit))
                .max()?;
            Some((position, value))
        })
        .max_by_key(|(_, value)| *value)
        .map(|(position, _)| position)
}

fn play_strength(cards: &[i32], rules: &TractorRules, lead_suit: Option<i32>) -> i32 {
    cards
        .iter()
        .map(|card| tractor_card_value(*card, rules, lead_suit))
        .max()
        .unwrap_or_default()
}

fn hand_candidates_for_count(hand: &[i32], count: usize) -> Vec<Vec<i32>> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return hand.iter().map(|card| vec![*card]).collect();
    }
    let mut by_rank: HashMap<i32, Vec<i32>> = HashMap::new();
    for card in hand {
        by_rank.entry(card_rank(*card)).or_default().push(*card);
    }
    let mut out = Vec::new();
    for cards in by_rank.values_mut() {
        cards.sort_unstable();
        if cards.len() >= count {
            out.push(cards[..count].to_vec());
        }
    }
    out
}

impl TractorGameState {
    pub fn active_positions(&self) -> Vec<usize> {
        let mut positions: Vec<_> = self.base.lock().unwrap().players.keys().copied().collect();
        positions.sort_unstable();
        positions
    }

    pub fn attacking_score(&self) -> i32 {
        let defenders = team_positions((self.dealer_position + 1) % 4);
        defenders
            .iter()
            .map(|position| {
                self.collected_scores
                    .get(position)
                    .copied()
                    .unwrap_or_default()
            })
            .sum()
    }

    fn deal_current_round(&mut self) -> Result<(), &'static str> {
        let positions = self.active_positions();
        if positions.len() != 4 {
            return Err("Tractor requires exactly 4 players");
        }

        let mut deck = build_tractor_deck_with_removed_ranks(
            self.rules.deck_count,
            self.rules.removed_rank_mask,
        );
        if deck.len() <= positions.len() {
            return Err("not enough cards");
        }
        deck.shuffle(&mut rand::rng());
        let max_bottom = deck.len().saturating_sub(positions.len());
        let minimum_bottom = min_bottom_card_count(self.rules.deck_count).min(max_bottom);
        self.rules.bottom_card_count = adjusted_bottom_card_count(
            deck.len(),
            positions.len(),
            self.rules.bottom_card_count,
            minimum_bottom,
        )
        .ok_or("invalid bottom card count")?;

        self.phase = TractorPhase::Deal;
        self.hands.clear();
        self.bottom_cards.clear();
        self.bottom_multiplier = 1;
        self.collected_scores.clear();
        self.last_trick_winner = None;
        self.current_trick.clear();
        self.trick_index = 0;
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
        self.phase = TractorPhase::Play;
        self.base.lock().unwrap().action_received = false;
        Ok(())
    }

    pub fn deal_new_round(&mut self, mut rules: TractorRules) -> Result<(), &'static str> {
        rules.deck_count = rules.deck_count.clamp(2, 4);
        rules.blood_score_per_unit = rules.blood_score_per_unit.max(1);
        let positions = self.active_positions();
        if positions.len() != 4 {
            return Err("Tractor requires exactly 4 players");
        }
        rules.target_rank = first_match_rank(rules.removed_rank_mask, rules.final_target_rank);
        self.rules = rules;
        self.dealer_position = positions[0];
        self.round_index = 0;
        self.deal_current_round()
    }

    pub fn from_common(base: Arc<Mutex<CommonGameState>>) -> Self {
        Self {
            base,
            phase: TractorPhase::Start,
            rules: TractorRules {
                blood_enabled: true,
                blood_score_per_unit: 40,
                blood_start_score: 80,
                bottom_card_count: 8,
                deck_count: 2,
                final_target_rank: TractorRank::A,
                removed_rank_mask: 0,
                target_rank: TractorRank::A,
            },
            hands: HashMap::new(),
            bottom_cards: Vec::new(),
            bottom_multiplier: 1,
            collected_scores: HashMap::new(),
            last_trick_winner: None,
            dealer_position: 0,
            current_position: 0,
            round_index: 0,
            trick_index: 0,
            current_trick: Vec::new(),
        }
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

    pub fn is_ai_controlled_position(&self, position: usize) -> bool {
        let base = self.base.lock().unwrap();
        base.is_ai_position(position) || base.is_away(position) || base.is_disconnected(position)
    }

    fn candidate_is_legal(&self, position: usize, cards: &[i32]) -> bool {
        if cards.is_empty() || !same_play_shape(cards) {
            return false;
        }
        let Some(hand) = self.hands.get(&position) else {
            return false;
        };
        let mut available = hand.clone();
        if remove_cards_from_hand(&mut available, cards).is_err() {
            return false;
        }
        if let Some(lead) = self.current_trick.first() {
            if cards.len() != lead.cards.len() {
                return false;
            }
            let lead_suit = play_suit(&lead.cards, &self.rules);
            if must_follow_play_suit(hand, lead_suit, lead.cards.len(), &self.rules)
                && !cards
                    .iter()
                    .all(|card| card_matches_play_suit(*card, lead_suit, &self.rules))
            {
                return false;
            }
        }
        true
    }

    fn candidate_would_win(&self, position: usize, cards: &[i32]) -> bool {
        let mut trick = self.current_trick.clone();
        trick.push(WsTractorPlayedCards {
            position: position as i32,
            name: String::new(),
            cards: cards.to_vec(),
        });
        trick_winner(&trick, &self.rules) == Some(position)
    }

    fn partner_still_to_play(&self, position: usize) -> bool {
        let partner = (position + 2) % 4;
        let positions = self.active_positions();
        let Some(mut cursor) = self.next_position(position) else {
            return false;
        };
        while self
            .current_trick
            .iter()
            .all(|played| played.position != cursor as i32)
        {
            if cursor == partner {
                return true;
            }
            let Some(next) = positions
                .iter()
                .position(|item| *item == cursor)
                .map(|idx| positions[(idx + 1) % positions.len()])
            else {
                return false;
            };
            cursor = next;
            if cursor == position {
                return false;
            }
        }
        false
    }

    pub fn choose_auto_play(&self, position: usize) -> Option<Vec<i32>> {
        let hand = self.hands.get(&position)?;
        if hand.is_empty() {
            return None;
        }
        if self.current_trick.is_empty() {
            return hand
                .iter()
                .max_by_key(|card| tractor_card_value(**card, &self.rules, None))
                .map(|card| vec![*card]);
        }

        let lead = self.current_trick.first()?;
        let lead_suit = play_suit(&lead.cards, &self.rules);
        let mut candidates: Vec<Vec<i32>> = hand_candidates_for_count(hand, lead.cards.len())
            .into_iter()
            .filter(|cards| self.candidate_is_legal(position, cards))
            .collect();
        if candidates.is_empty() {
            return None;
        }

        let current_winner = trick_winner(&self.current_trick, &self.rules);
        let partner_winning = current_winner
            .map(|winner| team_positions(position).contains(&winner))
            .unwrap_or(false);
        if partner_winning {
            candidates.sort_by_key(|cards| {
                (
                    played_score(cards),
                    play_strength(cards, &self.rules, lead_suit),
                )
            });
            return candidates.into_iter().next();
        }

        let mut winning_candidates: Vec<Vec<i32>> = candidates
            .iter()
            .filter(|cards| self.candidate_would_win(position, cards))
            .cloned()
            .collect();
        if !winning_candidates.is_empty() {
            winning_candidates.sort_by_key(|cards| play_strength(cards, &self.rules, lead_suit));
            return winning_candidates.into_iter().next();
        }

        if self.partner_still_to_play(position) {
            candidates.sort_by_key(|cards| {
                (
                    played_score(cards),
                    play_strength(cards, &self.rules, lead_suit),
                )
            });
            return candidates.into_iter().next_back();
        }

        candidates.sort_by_key(|cards| {
            (
                played_score(cards),
                play_strength(cards, &self.rules, lead_suit),
            )
        });
        candidates.into_iter().next()
    }

    pub fn next_target_rank(&self) -> Option<TractorRank> {
        next_match_rank(
            self.rules.target_rank,
            self.rules.removed_rank_mask,
            self.rules.final_target_rank,
        )
    }

    pub fn match_finished(&self) -> bool {
        self.next_target_rank().is_none()
    }

    pub fn advance_after_settlement(&mut self) -> Result<bool, &'static str> {
        if self.phase != TractorPhase::Settlement {
            return Err("not in settlement");
        }
        let Some(next_rank) = self.next_target_rank() else {
            return Ok(false);
        };
        let winners = self.winner_positions_usize();
        if !winners.contains(&self.dealer_position)
            && let Some(next_dealer) = winners.first().copied()
        {
            self.dealer_position = next_dealer;
        }
        self.rules.target_rank = next_rank;
        self.round_index += 1;
        self.deal_current_round()?;
        Ok(true)
    }

    pub fn play_cards(
        &mut self,
        position: usize,
        name: String,
        cards: Vec<i32>,
    ) -> Result<WsTractorPlayedCards, &'static str> {
        if self.phase != TractorPhase::Play || self.current_position != position || cards.is_empty()
        {
            return Err("not current turn");
        }
        if !same_play_shape(&cards) {
            return Err("invalid play shape");
        }
        if let Some(lead) = self.current_trick.first()
            && cards.len() != lead.cards.len()
        {
            return Err("must follow card count");
        }
        if let Some(lead) = self.current_trick.first() {
            let lead_suit = play_suit(&lead.cards, &self.rules);
            let hand = self.hands.get(&position).cloned().unwrap_or_default();
            if must_follow_play_suit(&hand, lead_suit, lead.cards.len(), &self.rules)
                && !cards
                    .iter()
                    .all(|card| card_matches_play_suit(*card, lead_suit, &self.rules))
            {
                return Err("must follow suit");
            }
        }
        remove_cards_from_hand(self.hands.entry(position).or_default(), &cards)?;
        let played = WsTractorPlayedCards {
            position: position as i32,
            name,
            cards,
        };
        self.current_trick.push(played.clone());
        if self.current_trick.len() >= self.active_positions().len() {
            let trick_score: i32 = self
                .current_trick
                .iter()
                .map(|played| played_score(&played.cards))
                .sum();
            let winner = trick_winner(&self.current_trick, &self.rules).unwrap_or(position);
            *self.collected_scores.entry(winner).or_default() += trick_score;
            self.last_trick_winner = Some(winner);
            self.current_trick.clear();
            self.trick_index += 1;
            self.current_position = winner;
        } else {
            self.current_position = self.next_position(position).unwrap_or(position);
        }
        if self.is_finished() {
            if let Some(last_winner) = self.last_trick_winner {
                let bottom_score = played_score(&self.bottom_cards) * self.bottom_multiplier;
                *self.collected_scores.entry(last_winner).or_default() += bottom_score;
            }
            self.phase = TractorPhase::Settlement;
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

    pub fn settlement_score(&self) -> i32 {
        let attacking_score = self.attacking_score();
        if attacking_score >= self.rules.blood_start_score {
            attacking_score
        } else {
            (self.rules.blood_start_score - attacking_score).max(1)
        }
    }

    pub fn snapshot(&self) -> WsTractorTableSnapshotEvent {
        WsTractorTableSnapshotEvent {
            phase: self.phase,
            deck_count: self.rules.deck_count as i32,
            target_rank: self.rules.target_rank,
            final_target_rank: self.rules.final_target_rank,
            removed_rank_mask: self.rules.removed_rank_mask,
            round_index: self.round_index,
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

    pub fn winner_positions_usize(&self) -> Vec<usize> {
        let attacking_score = self.attacking_score();
        let winners = if attacking_score >= self.rules.blood_start_score {
            team_positions((self.dealer_position + 1) % 4)
        } else {
            team_positions(self.dealer_position)
        };
        winners.to_vec()
    }

    pub fn winner_positions(&self) -> Vec<i32> {
        self.winner_positions_usize()
            .iter()
            .map(|position| *position as i32)
            .collect()
    }
}

impl GameState for TractorGameState {
    fn can_accept_players(&self) -> bool {
        self.phase == TractorPhase::Start
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.base)
    }
}

impl TractorRules {
    pub fn blood_units(&self, score: i32) -> i32 {
        if !self.blood_enabled || score < self.blood_start_score {
            return 0;
        }
        ((score - self.blood_start_score) / self.blood_score_per_unit.max(1)) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjusted_bottom_keeps_all_hands_equal() {
        for deck_count in 2..=4 {
            let total = build_tractor_deck(deck_count).len();
            let bottom =
                adjusted_bottom_card_count(total, 4, 8, min_bottom_card_count(deck_count)).unwrap();
            assert_eq!((total - bottom) % 4, 0);
            assert!(bottom >= min_bottom_card_count(deck_count));
        }
    }

    #[test]
    fn blood_units_start_after_threshold() {
        let rules = TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            removed_rank_mask: 0,
            target_rank: TractorRank::A,
        };
        assert_eq!(rules.blood_units(79), 0);
        assert_eq!(rules.blood_units(80), 1);
        assert_eq!(rules.blood_units(120), 2);
    }

    #[test]
    fn rank_path_skips_removed_ranks_and_keeps_final_target() {
        let removed = tractor_rank_mask(&[
            TractorRank::THREE,
            TractorRank::FOUR,
            TractorRank::SIX,
            TractorRank::SEVEN,
        ]);
        assert_eq!(
            tractor_rank_path(removed, TractorRank::NINE),
            vec![
                TractorRank::TWO,
                TractorRank::FIVE,
                TractorRank::EIGHT,
                TractorRank::NINE
            ]
        );
    }

    #[test]
    fn play_rejects_wrong_card_count_and_must_follow_suit() {
        let mut state = test_state();
        state.hands.insert(0, vec![1, 101]);
        state.hands.insert(1, vec![2, 15, 102]);

        state
            .play_cards(0, "u0".to_owned(), vec![1, 101])
            .expect("lead pair");
        assert!(state.play_cards(1, "u1".to_owned(), vec![2]).is_err());
        assert!(state.play_cards(1, "u1".to_owned(), vec![2, 15]).is_err());
        state
            .play_cards(1, "u1".to_owned(), vec![2, 102])
            .expect("follow lead suit pair");
    }

    fn test_state() -> TractorGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state = TractorGameState::from_common(Arc::new(Mutex::new(common)));
        state.phase = TractorPhase::Play;
        state.rules = TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            removed_rank_mask: 0,
            target_rank: TractorRank::A,
        };
        state.dealer_position = 0;
        state.current_position = 0;
        state
    }

    #[test]
    fn three_decks_uses_at_least_ten_bottom_cards() {
        let total = build_tractor_deck(3).len();
        let bottom = adjusted_bottom_card_count(total, 4, 8, min_bottom_card_count(3)).unwrap();
        assert_eq!(bottom, 10);
        assert_eq!((total - bottom) % 4, 0);
    }

    #[test]
    fn trick_winner_collects_score_and_leads_next_trick() {
        let mut state = test_state();
        state.hands.insert(0, vec![4]);
        state.hands.insert(1, vec![5]);
        state.hands.insert(2, vec![6]);
        state.hands.insert(3, vec![7]);

        state.play_cards(0, "u0".to_owned(), vec![4]).unwrap();
        state.play_cards(1, "u1".to_owned(), vec![5]).unwrap();
        state.play_cards(2, "u2".to_owned(), vec![6]).unwrap();
        state.play_cards(3, "u3".to_owned(), vec![7]).unwrap();

        assert_eq!(state.trick_index, 1);
        assert_eq!(state.current_position, 3);
        assert_eq!(state.collected_scores.get(&3).copied(), Some(5));
    }

    #[test]
    fn ai_following_opponent_prefers_smallest_winning_card() {
        let mut state = test_state();
        state.current_position = 1;
        state.current_trick.push(WsTractorPlayedCards {
            position: 0,
            name: "u0".to_owned(),
            cards: vec![4],
        });
        state.hands.insert(1, vec![5, 6, 13]);

        assert_eq!(state.choose_auto_play(1), Some(vec![5]));
    }

    #[test]
    fn trump_beats_lead_suit_and_attacking_team_can_win() {
        let mut state = test_state();
        state.hands.insert(0, vec![4]);
        state.hands.insert(1, vec![13]);
        state.hands.insert(2, vec![5]);
        state.hands.insert(3, vec![6]);
        state.bottom_cards = vec![4, 9, 12, 109, 112, 209, 212, 309];

        state.play_cards(0, "u0".to_owned(), vec![4]).unwrap();
        state.play_cards(1, "u1".to_owned(), vec![13]).unwrap();
        state.play_cards(2, "u2".to_owned(), vec![5]).unwrap();
        state.play_cards(3, "u3".to_owned(), vec![6]).unwrap();

        assert_eq!(state.phase, TractorPhase::Settlement);
        assert_eq!(state.last_trick_winner, Some(1));
        assert_eq!(state.attacking_score(), 80);
        assert_eq!(state.winner_positions(), vec![1, 3]);
        assert_eq!(state.settlement_score(), 80);
    }

    #[test]
    fn settlement_advances_rank_until_final_target() {
        let removed = tractor_rank_mask(&[
            TractorRank::THREE,
            TractorRank::FOUR,
            TractorRank::SIX,
            TractorRank::SEVEN,
        ]);
        let mut state = test_state();
        state.rules.final_target_rank = TractorRank::NINE;
        state.rules.removed_rank_mask = removed;
        state.rules.target_rank = TractorRank::EIGHT;
        state.phase = TractorPhase::Settlement;

        assert_eq!(state.next_target_rank(), Some(TractorRank::NINE));
        assert!(state.advance_after_settlement().expect("advance"));
        assert_eq!(state.rules.target_rank, TractorRank::NINE);

        state.phase = TractorPhase::Settlement;
        assert!(state.match_finished());
        assert_eq!(state.next_target_rank(), None);
        assert!(!state.advance_after_settlement().expect("finished"));
    }
}
