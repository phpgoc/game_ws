use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use rand::{Rng, seq::SliceRandom};
use share_type_public::{
    TractorPhase, TractorRank, TractorSuit, WsTractorFailedThrowEvent, WsTractorPlayedCards,
    WsTractorPlayerHandCount, WsTractorTableSnapshotEvent, WsTractorTrumpDeclaration,
};
use upgrade_common::{
    Card, Rank, ScoreOutcome, ScoreProgression, Suit, card_is_trump, compact_plain_rank_position,
    next_four_player_dealer, next_level_rank, trump_order_position,
};
use ws_common::{CommonGameState, GameState};

use crate::combo::{self, Combo};

pub const TRACTOR_RANKS: [TractorRank; 12] = [
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

pub const MIN_TRACTOR_DECK_COUNT: usize = 2;
pub const MAX_TRACTOR_DECK_COUNT: usize = 3;

#[derive(Debug)]
pub struct TractorGameState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: TractorPhase,
    pub rules: TractorRules,
    pub team_target_ranks: [TractorRank; 2],
    pub hands: HashMap<usize, Vec<i32>>,
    pub deal_queue: VecDeque<(usize, i32)>,
    pub dealt_count: usize,
    pub total_deal_count: usize,
    pub bottom_cards: Vec<i32>,
    pub declaration: Option<WsTractorTrumpDeclaration>,
    pub bottom_multiplier: i32,
    pub collected_scores: HashMap<usize, i32>,
    pub player_scores: HashMap<usize, i32>,
    pub last_trick_winner: Option<usize>,
    pub dealer_position: usize,
    pub current_position: usize,
    pub round_index: i32,
    pub trick_index: i32,
    /// Completed tricks are public table information. Keeping them lets an AI
    /// remember exposed cards and infer which players have exhausted a suit
    /// without inspecting anyone's hidden hand.
    pub completed_tricks: Vec<Vec<WsTractorPlayedCards>>,
    pub current_trick: Vec<WsTractorPlayedCards>,
    /// A failed throw is public table information even though only its weakest
    /// component is actually played. Keep the attempted cards and the play
    /// sequence so the official AI can remember the exposed holding without
    /// treating cards that were later played as still hidden in that hand.
    pub failed_throws: Vec<TractorFailedThrow>,
    play_count: usize,
}

#[derive(Debug, Clone)]
pub struct TractorFailedThrow {
    pub position: usize,
    pub attempted_cards: Vec<i32>,
    pub played_cards: Vec<i32>,
    pub play_sequence: usize,
}

#[derive(Debug, Clone)]
pub struct TractorRules {
    pub attacking_win_score: i32,
    pub score_per_level: i32,
    pub shutout_bonus_levels: u8,
    pub bottom_card_count: usize,
    pub deck_count: usize,
    pub final_target_rank: TractorRank,
    pub target_rank: TractorRank,
    pub trump_suit: Option<TractorSuit>,
}

pub type TractorStateHandle = Arc<Mutex<TractorGameState>>;

pub(crate) fn base_card(card: i32) -> i32 {
    i32::from(decoded_card(card).identity())
}

fn decoded_card(card: i32) -> Card {
    Card::try_from(card).expect("tractor state only contains valid card ids")
}

pub fn build_tractor_deck(deck_count: usize) -> Vec<i32> {
    let deck_count = deck_count.clamp(MIN_TRACTOR_DECK_COUNT, MAX_TRACTOR_DECK_COUNT);
    let mut cards = Vec::with_capacity(deck_count * 54);
    for deck_index in 0..deck_count {
        let offset = deck_index as i32 * 100;
        for card in 1..=54 {
            let full_card = offset + card;
            cards.push(full_card);
        }
    }
    cards
}

fn candidate_in_hand(hand: &[i32], cards: &[i32]) -> bool {
    let mut available = hand.to_vec();
    remove_cards_from_hand(&mut available, cards).is_ok()
}

pub(crate) fn card_rank(card: i32) -> i32 {
    decoded_card(card).rank() as i32
}

pub(crate) fn card_score(card: i32) -> i32 {
    i32::from(decoded_card(card).points())
}

pub(crate) fn card_suit(card: i32) -> Option<i32> {
    decoded_card(card).suit().map(|suit| suit as i32)
}

pub(crate) fn is_trump_card(card: i32, rules: &TractorRules) -> bool {
    card_is_trump(
        decoded_card(card),
        common_rank(rules.target_rank),
        rules.trump_suit.map(common_suit),
    )
}

pub(crate) fn tractor_card_position(card: i32, rules: &TractorRules) -> i32 {
    let card = decoded_card(card);
    trump_order_position(
        card,
        common_rank(rules.target_rank),
        rules.trump_suit.map(common_suit),
    )
    .or_else(|| compact_plain_rank_position(card.rank(), common_rank(rules.target_rank)))
    .unwrap_or(card.rank() as i32)
}

pub fn standard_bottom_card_count(deck_count: usize) -> usize {
    match deck_count {
        3 => 6,
        2 => 8,
        _ => 8,
    }
}

fn next_match_rank(
    current_rank: TractorRank,
    final_target_rank: TractorRank,
    levels: usize,
) -> Option<TractorRank> {
    next_level_rank(
        common_rank(current_rank),
        common_rank(final_target_rank),
        &[],
        levels,
    )
    .and_then(tractor_rank)
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

/// Two seats are partners when they sit across from each other (0&2, 1&3).
#[cfg(feature = "official")]
pub(crate) fn same_team(a: usize, b: usize) -> bool {
    a % 2 == b % 2
}

fn team_positions(position: usize) -> [usize; 2] {
    [position, (position + 2) % 4]
}

pub(crate) fn tractor_card_value(card: i32, rules: &TractorRules, lead_suit: Option<i32>) -> i32 {
    let rank = card_rank(card);
    if is_trump_card(card, rules) {
        return 1_000 + tractor_card_position(card, rules);
    }
    if card_suit(card) == lead_suit {
        return 500 + rank;
    }
    rank
}

pub fn tractor_rank_from_setting_index(index: i32) -> TractorRank {
    TRACTOR_RANKS
        .get(index.clamp(0, TRACTOR_RANKS.len() as i32 - 1) as usize)
        .copied()
        .unwrap_or(TractorRank::A)
}

fn common_rank(rank: TractorRank) -> Rank {
    match rank {
        TractorRank::TWO => Rank::Two,
        TractorRank::THREE => Rank::Three,
        TractorRank::FOUR => Rank::Four,
        TractorRank::FIVE => Rank::Five,
        TractorRank::SIX => Rank::Six,
        TractorRank::SEVEN => Rank::Seven,
        TractorRank::EIGHT => Rank::Eight,
        TractorRank::NINE => Rank::Nine,
        TractorRank::TEN => Rank::Ten,
        TractorRank::J => Rank::Jack,
        TractorRank::Q => Rank::Queen,
        TractorRank::K => Rank::King,
        TractorRank::A => Rank::Ace,
    }
}

fn common_suit(suit: TractorSuit) -> Suit {
    match suit {
        TractorSuit::SPADE => Suit::Spade,
        TractorSuit::HEART => Suit::Heart,
        TractorSuit::CLUB => Suit::Club,
        TractorSuit::DIAMOND => Suit::Diamond,
    }
}

fn tractor_rank(rank: Rank) -> Option<TractorRank> {
    match rank {
        Rank::Two => Some(TractorRank::TWO),
        Rank::Three => Some(TractorRank::THREE),
        Rank::Four => Some(TractorRank::FOUR),
        Rank::Five => Some(TractorRank::FIVE),
        Rank::Six => Some(TractorRank::SIX),
        Rank::Seven => Some(TractorRank::SEVEN),
        Rank::Eight => Some(TractorRank::EIGHT),
        Rank::Nine => Some(TractorRank::NINE),
        Rank::Ten => Some(TractorRank::TEN),
        Rank::Jack => Some(TractorRank::J),
        Rank::Queen => Some(TractorRank::Q),
        Rank::King => Some(TractorRank::K),
        Rank::Ace => Some(TractorRank::A),
        Rank::SmallJoker | Rank::BigJoker => None,
    }
}

pub(crate) fn tractor_suit_from_index(suit: i32) -> Option<TractorSuit> {
    match suit {
        0 => Some(TractorSuit::SPADE),
        1 => Some(TractorSuit::HEART),
        2 => Some(TractorSuit::CLUB),
        3 => Some(TractorSuit::DIAMOND),
        _ => None,
    }
}

impl TractorGameState {
    pub fn active_positions(&self) -> Vec<usize> {
        let mut positions: Vec<_> = self.base.lock().unwrap().players.keys().copied().collect();
        positions.sort_unstable();
        positions
    }

    pub fn advance_after_settlement(&mut self) -> Result<bool, &'static str> {
        self.advance_after_settlement_with_rng(&mut rand::rng())
    }

    pub fn advance_after_settlement_with_rng<R: Rng + ?Sized>(
        &mut self,
        rng: &mut R,
    ) -> Result<bool, &'static str> {
        if self.phase != TractorPhase::Settlement {
            return Err("not in settlement");
        }
        let Some(next_rank) = self.next_target_rank() else {
            return Ok(false);
        };
        let outcome = self.score_outcome();
        let winners = self.winner_positions_usize();
        let winning_team = winners[0] % 2;
        self.team_target_ranks[winning_team] = next_rank;
        self.dealer_position = next_four_player_dealer(self.dealer_position, outcome.side);
        self.rules.target_rank = next_rank;
        self.round_index += 1;
        self.deal_current_round_with_rng(rng)?;
        Ok(true)
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

    pub fn auto_declaration_cards(&self, position: usize) -> Option<Vec<i32>> {
        let current_strength = self
            .declaration
            .as_ref()
            .map(|declaration| declaration.strength)
            .unwrap_or_default();
        crate::ai::declaration_decision(self, position, current_strength, false)
            .map(|decision| decision.cards)
    }

    pub fn bury_bottom(&mut self, position: usize, cards: Vec<i32>) -> Result<(), &'static str> {
        if self.phase != TractorPhase::Bury || position != self.dealer_position {
            return Err("not dealer bury turn");
        }
        if cards.len() != self.rules.bottom_card_count {
            return Err("wrong bottom card count");
        }
        if self.round_index > 0 && self.rules.trump_suit.is_none() {
            return Err("dealer must select trump first");
        }
        remove_cards_from_hand(self.hands.entry(position).or_default(), &cards)?;
        self.bottom_cards = cards;
        self.phase = TractorPhase::Play;
        self.current_position = self.dealer_position;
        self.base.lock().unwrap().action_received = false;
        Ok(())
    }

    fn candidate_would_win(&self, position: usize, cards: &[i32]) -> bool {
        let mut trick = self.current_trick.clone();
        trick.push(WsTractorPlayedCards {
            position: position as i32,
            name: String::new(),
            cards: cards.to_vec(),
        });
        combo::trick_winner(&trick, &self.rules) == Some(position)
    }

    pub fn choose_auto_bury(&self) -> Option<Vec<i32>> {
        if self.phase != TractorPhase::Bury {
            return None;
        }
        crate::ai::choose_bury(self)
    }

    pub fn choose_timeout_bury(&self) -> Option<Vec<i32>> {
        if self.phase != TractorPhase::Bury {
            return None;
        }
        let mut hand = self.hands.get(&self.dealer_position)?.clone();
        hand.sort_by_key(|card| tractor_card_value(*card, &self.rules, None));
        (hand.len() >= self.rules.bottom_card_count).then(|| {
            hand.into_iter()
                .take(self.rules.bottom_card_count)
                .collect()
        })
    }

    /// A deliberately simple, rules-correct play used by ordinary AWAY seats.
    ///
    /// It never initiates a tractor or throw: on lead it plays the highest
    /// non-scoring side-suit single (A, then Q, and so on, skipping K/10/5).
    /// While following, it feeds points when the partner is currently winning,
    /// uses the cheapest legal winner against an opponent, and otherwise keeps
    /// as many point cards as the follow rules allow.
    pub fn choose_away_play(&self, position: usize) -> Option<Vec<i32>> {
        let hand = self.hands.get(&position)?;
        if hand.is_empty() {
            return None;
        }
        let Some(lead) = self.lead_combo() else {
            if let Some(card) = hand
                .iter()
                .filter(|card| !is_trump_card(**card, &self.rules) && card_score(**card) == 0)
                .max_by_key(|card| (card_rank(**card), -**card))
            {
                return Some(vec![*card]);
            }

            // If every side-suit card carries points, avoid leading those
            // points while any non-scoring trump remains. Only lead a point
            // card when the whole hand forces it.
            return hand
                .iter()
                .filter(|card| card_score(**card) == 0)
                .min_by_key(|card| (tractor_card_value(**card, &self.rules, None), **card))
                .or_else(|| {
                    hand.iter().min_by_key(|card| {
                        (
                            card_score(**card),
                            tractor_card_value(**card, &self.rules, None),
                            **card,
                        )
                    })
                })
                .map(|card| vec![*card]);
        };

        let lead_suit = lead.suit;
        let strength = |cards: &[i32]| {
            cards
                .iter()
                .map(|card| tractor_card_value(*card, &self.rules, lead_suit))
                .max()
                .unwrap_or_default()
        };
        let mut candidates = self.legal_follows(position, &lead);
        if candidates.is_empty() {
            return combo::forced_follow(hand, &lead, &self.rules);
        }

        let current_winner = combo::trick_winner(&self.current_trick, &self.rules);
        let partner_winning = current_winner
            .map(|winner| team_positions(position).contains(&winner) && winner != position)
            .unwrap_or(false);

        if partner_winning {
            candidates.sort_by_key(|cards| (-played_score(cards), strength(cards)));
            return candidates.into_iter().next();
        }

        let mut winning: Vec<Vec<i32>> = candidates
            .iter()
            .filter(|cards| self.candidate_would_win(position, cards))
            .cloned()
            .collect();
        if !winning.is_empty() {
            winning.sort_by_key(|cards| strength(cards));
            return winning.into_iter().next();
        }

        // The opponent is still winning and no legal reply can overtake it.
        candidates.sort_by_key(|cards| (played_score(cards), strength(cards)));
        candidates.into_iter().next()
    }

    /// A safe, rules-correct fallback for native and member-takeover AI.
    /// Leads the lowest single; when following, beats an opponent with the
    /// smallest winning play, feeds points to a winning partner only when safe,
    /// and otherwise sheds the lowest legal cards.
    pub fn choose_auto_play(&self, position: usize) -> Option<Vec<i32>> {
        let hand = self.hands.get(&position)?;
        if hand.is_empty() {
            return None;
        }
        let Some(lead) = self.lead_combo() else {
            return hand
                .iter()
                .min_by_key(|card| tractor_card_value(**card, &self.rules, None))
                .map(|card| vec![*card]);
        };

        let lead_suit = lead.suit;
        let strength = |cards: &[i32]| {
            cards
                .iter()
                .map(|card| tractor_card_value(*card, &self.rules, lead_suit))
                .max()
                .unwrap_or_default()
        };
        let mut candidates = self.legal_follows(position, &lead);
        if candidates.is_empty() {
            return combo::forced_follow(hand, &lead, &self.rules);
        }

        let current_winner = combo::trick_winner(&self.current_trick, &self.rules);
        let partner_winning = current_winner
            .map(|winner| team_positions(position).contains(&winner) && winner != position)
            .unwrap_or(false);
        let is_last_to_play = self.current_trick.len() + 1 >= self.active_positions().len();

        if !partner_winning {
            let mut winning: Vec<Vec<i32>> = candidates
                .iter()
                .filter(|cards| self.candidate_would_win(position, cards))
                .cloned()
                .collect();
            if !winning.is_empty() {
                winning.sort_by_key(|cards| strength(cards));
                let cheapest = &winning[0];
                let ruffing = lead.suit.is_some()
                    && cheapest
                        .iter()
                        .all(|card| is_trump_card(*card, &self.rules));
                let worth_taking = combo::trick_points(&self.current_trick) > 0;
                if !(ruffing && !worth_taking && self.partner_still_to_play(position)) {
                    return winning.into_iter().next();
                }
            }
        }

        candidates.sort_by_key(|cards| {
            if partner_winning && is_last_to_play {
                (-played_score(cards), strength(cards))
            } else {
                (played_score(cards), strength(cards))
            }
        });
        candidates.into_iter().next()
    }

    fn deal_current_round_with_rng<R: Rng + ?Sized>(
        &mut self,
        rng: &mut R,
    ) -> Result<(), &'static str> {
        let positions = self.active_positions();
        if positions.len() != 4 {
            return Err("Tractor requires exactly 4 players");
        }

        let mut deck = build_tractor_deck(self.rules.deck_count);
        if deck.len() <= positions.len() {
            return Err("not enough cards");
        }
        deck.shuffle(rng);
        self.rules.bottom_card_count = standard_bottom_card_count(self.rules.deck_count);
        if self.rules.bottom_card_count >= deck.len()
            || !(deck.len() - self.rules.bottom_card_count).is_multiple_of(positions.len())
        {
            return Err("invalid standard bottom card count");
        }

        self.phase = TractorPhase::Deal;
        self.hands = self
            .active_positions()
            .into_iter()
            .map(|position| (position, Vec::new()))
            .collect();
        self.deal_queue.clear();
        self.dealt_count = 0;
        self.total_deal_count = 0;
        self.bottom_cards.clear();
        self.declaration = None;
        self.rules.trump_suit = None;
        self.bottom_multiplier = 1;
        self.collected_scores.clear();
        self.last_trick_winner = None;
        self.completed_tricks.clear();
        self.current_trick.clear();
        self.failed_throws.clear();
        self.play_count = 0;
        self.trick_index = 0;
        self.current_position = self.dealer_position;

        for _ in 0..self.rules.bottom_card_count {
            if let Some(card) = deck.pop() {
                self.bottom_cards.push(card);
            }
        }
        for (idx, card) in deck.into_iter().enumerate() {
            let position = positions[idx % positions.len()];
            self.deal_queue.push_back((position, card));
        }
        self.total_deal_count = self.deal_queue.len();
        for position in positions {
            self.hands.insert(position, Vec::new());
        }
        self.base.lock().unwrap().action_received = false;
        Ok(())
    }

    pub fn deal_new_round(&mut self, rules: TractorRules) -> Result<(), &'static str> {
        self.deal_new_round_with_rng(rules, &mut rand::rng())
    }

    pub fn deal_new_round_with_rng<R: Rng + ?Sized>(
        &mut self,
        mut rules: TractorRules,
        rng: &mut R,
    ) -> Result<(), &'static str> {
        rules.deck_count = rules
            .deck_count
            .clamp(MIN_TRACTOR_DECK_COUNT, MAX_TRACTOR_DECK_COUNT);
        rules.attacking_win_score = rules.attacking_win_score.max(1);
        rules.score_per_level = rules.score_per_level.max(1);
        let positions = self.active_positions();
        if positions.len() != 4 {
            return Err("Tractor requires exactly 4 players");
        }
        rules.target_rank = TractorRank::THREE;
        rules.trump_suit = None;
        self.rules = rules;
        self.team_target_ranks = [TractorRank::THREE; 2];
        self.dealer_position = positions[0];
        self.round_index = 0;
        self.deal_current_round_with_rng(rng)
    }

    fn best_ai_declaration(&self, forced: bool) -> Option<(usize, crate::ai::DeclarationDecision)> {
        let current_strength = if forced {
            0
        } else {
            self.declaration
                .as_ref()
                .map(|declaration| declaration.strength)
                .unwrap_or_default()
        };
        self.active_positions()
            .into_iter()
            .filter(|position| self.is_ai_controlled_position(*position))
            .filter_map(|position| {
                crate::ai::declaration_decision(self, position, current_strength, forced)
                    .map(|decision| (position, decision))
            })
            .max_by(|(_, left), (_, right)| {
                left.cards
                    .len()
                    .cmp(&right.cards.len())
                    .then_with(|| {
                        left.assessment
                            .success_probability
                            .total_cmp(&right.assessment.success_probability)
                    })
                    .then_with(|| left.assessment.score.cmp(&right.assessment.score))
            })
    }

    /// Deal exactly one public-progress/private-card step. During the first
    /// deal, an AI may declare as soon as its currently dealt hand clears the
    /// normal threshold. The final step moves the table into Bury and applies
    /// the marginal fallback when nobody has declared.
    pub fn deal_next_card(
        &mut self,
    ) -> Option<(usize, i32, bool, Option<WsTractorTrumpDeclaration>)> {
        if self.phase != TractorPhase::Deal {
            return None;
        }
        let (position, card) = self.deal_queue.pop_front()?;
        let hand = self.hands.entry(position).or_default();
        hand.push(card);
        hand.sort_by_key(|card| tractor_card_value(*card, &self.rules, None));
        self.dealt_count += 1;
        let finished = self.deal_queue.is_empty();
        let mut auto_declaration = None;
        if self.round_index == 0 {
            let best_ai_declaration = self.best_ai_declaration(false).or_else(|| {
                (finished && self.declaration.is_none())
                    .then(|| self.best_ai_declaration(true))
                    .flatten()
            });
            if let Some((position, decision)) = best_ai_declaration {
                auto_declaration = self.declare_trump(position, decision.cards).ok();
            }
        }
        if finished {
            if self.round_index == 0 && self.declaration.is_none() {
                let fallback = self.active_positions().into_iter().find_map(|position| {
                    let card = self.hands.get(&position)?.iter().copied().find(|card| {
                        card_rank(*card) == self.rules.target_rank as i32
                            && card_suit(*card).is_some()
                    })?;
                    self.declare_trump(position, vec![card]).ok()
                });
                auto_declaration = fallback;
            }
            if self.round_index == 0 && self.declaration.is_none() {
                auto_declaration = self.declare_bottom_level_fallback();
            }
            if self.round_index == 0
                && let Some(declaration) = &self.declaration
            {
                self.dealer_position = declaration.position as usize;
            }
            self.current_position = self.dealer_position;
            self.hands
                .entry(self.dealer_position)
                .or_default()
                .extend(self.bottom_cards.iter().copied());
            if let Some(dealer_hand) = self.hands.get_mut(&self.dealer_position) {
                dealer_hand.sort_by_key(|card| tractor_card_value(*card, &self.rules, None));
            }
            self.phase = TractorPhase::Bury;
            self.base.lock().unwrap().action_received = false;
        }
        Some((position, card, finished, auto_declaration))
    }

    fn declare_bottom_level_fallback(&mut self) -> Option<WsTractorTrumpDeclaration> {
        let position = self.active_positions().first().copied()?;
        let card = self.bottom_cards.iter().copied().find(|card| {
            card_rank(*card) == self.rules.target_rank as i32 && card_suit(*card).is_some()
        })?;
        let suit = card_suit(card).and_then(tractor_suit_from_index)?;
        let declaration = WsTractorTrumpDeclaration {
            position: position as i32,
            name: self.player_name(position),
            cards: vec![card],
            trump_suit: suit,
            strength: 1,
            target_rank: self.rules.target_rank,
        };
        self.rules.trump_suit = Some(suit);
        self.dealer_position = position;
        self.current_position = position;
        self.declaration = Some(declaration.clone());
        Some(declaration)
    }

    pub fn dealer_bottom_cards(&self) -> Option<Vec<i32>> {
        (self.phase == TractorPhase::Bury).then(|| self.bottom_cards.clone())
    }

    pub fn declare_trump(
        &mut self,
        position: usize,
        cards: Vec<i32>,
    ) -> Result<WsTractorTrumpDeclaration, &'static str> {
        if self.round_index != 0 || self.phase != TractorPhase::Deal || cards.is_empty() {
            return Err("not in deal phase");
        }
        let hand = self.hands.get(&position).cloned().unwrap_or_default();
        if !candidate_in_hand(&hand, &cards) {
            return Err("declaration card not dealt");
        }
        let first_base = base_card(cards[0]);
        let Some(suit) = card_suit(cards[0]).and_then(tractor_suit_from_index) else {
            return Err("joker cannot declare trump");
        };
        if cards.iter().any(|card| {
            base_card(*card) != first_base || card_rank(*card) != self.rules.target_rank as i32
        }) {
            return Err("declaration must use identical level cards");
        }
        let strength = cards.len() as i32;
        if self
            .declaration
            .as_ref()
            .is_some_and(|current| current.strength >= strength)
        {
            return Err("declaration is not stronger");
        }
        let declaration = WsTractorTrumpDeclaration {
            position: position as i32,
            name: self.player_name(position),
            cards,
            trump_suit: suit,
            strength,
            target_rank: self.rules.target_rank,
        };
        self.rules.trump_suit = Some(suit);
        if self.round_index == 0 {
            self.dealer_position = position;
            self.current_position = position;
        }
        self.declaration = Some(declaration.clone());
        Ok(declaration)
    }

    fn failed_throw_component(&self, position: usize, cards: &[i32]) -> Option<Vec<i32>> {
        let components = combo::throw_components(cards, &self.rules)?;
        components
            .into_iter()
            .filter(|component| {
                let Some(lead) = combo::classify(component, &self.rules) else {
                    return false;
                };
                let Some(value) = combo::combo_win_value(component, &lead, &self.rules) else {
                    return false;
                };
                self.active_positions()
                    .into_iter()
                    // Throw validation is a table rule, not a team-control
                    // decision: a higher component at any of the other three
                    // seats makes the proposed throw fail.
                    .filter(|other| *other != position)
                    .flat_map(|other| self.legal_follows(other, &lead))
                    // A throw is challenged only by a higher component in the
                    // same lead group. A player who is void may ruff the
                    // accepted trick, but that trump play does not make the
                    // original plain-suit throw fail at declaration time.
                    .filter(|reply| {
                        reply
                            .iter()
                            .all(|card| combo::card_in_group(*card, lead.suit, &self.rules))
                    })
                    .filter_map(|reply| combo::combo_win_value(&reply, &lead, &self.rules))
                    .any(|reply_value| reply_value > value)
            })
            .min_by_key(|component| {
                let lead = combo::classify(component, &self.rules)
                    .expect("throw component remains classifiable");
                combo::combo_win_value(component, &lead, &self.rules).unwrap_or_default()
            })
    }

    pub fn from_common(base: Arc<Mutex<CommonGameState>>) -> Self {
        Self {
            base,
            phase: TractorPhase::Start,
            rules: TractorRules {
                attacking_win_score: 80,
                score_per_level: 40,
                shutout_bonus_levels: 1,
                bottom_card_count: 8,
                deck_count: 2,
                final_target_rank: TractorRank::A,
                target_rank: TractorRank::A,
                trump_suit: None,
            },
            team_target_ranks: [TractorRank::THREE; 2],
            hands: HashMap::new(),
            deal_queue: VecDeque::new(),
            dealt_count: 0,
            total_deal_count: 0,
            bottom_cards: Vec::new(),
            declaration: None,
            bottom_multiplier: 1,
            collected_scores: HashMap::new(),
            player_scores: HashMap::new(),
            last_trick_winner: None,
            dealer_position: 0,
            current_position: 0,
            round_index: 0,
            trick_index: 0,
            completed_tricks: Vec::new(),
            current_trick: Vec::new(),
            failed_throws: Vec::new(),
            play_count: 0,
        }
    }

    pub fn hand_count(&self) -> usize {
        if self.total_deal_count > 0 {
            self.total_deal_count / self.active_positions().len().max(1)
        } else {
            self.hands.values().map(Vec::len).max().unwrap_or_default()
        }
    }

    /// 将已经结束的整场比赛收敛回可再次开始的房间状态。
    ///
    /// 结算事件仍然保留给客户端展示；清理前先发送这个空桌快照，
    /// 让客户端知道服务端已经停止本场并可以重新 START。
    pub fn reset_to_lobby(&mut self) {
        self.phase = TractorPhase::Start;
        self.rules.target_rank = TractorRank::THREE;
        self.rules.trump_suit = None;
        self.team_target_ranks = [TractorRank::THREE; 2];
        self.hands.clear();
        self.deal_queue.clear();
        self.dealt_count = 0;
        self.total_deal_count = 0;
        self.bottom_cards.clear();
        self.declaration = None;
        self.bottom_multiplier = 1;
        self.collected_scores.clear();
        self.player_scores.clear();
        self.last_trick_winner = None;
        self.dealer_position = 0;
        self.current_position = 0;
        self.round_index = 0;
        self.trick_index = 0;
        self.completed_tricks.clear();
        self.current_trick.clear();
        self.failed_throws.clear();
        self.play_count = 0;
        self.set_turn_countdown(0);
        self.base.lock().unwrap().action_received = false;
    }

    pub fn is_ai_controlled_position(&self, position: usize) -> bool {
        let base = self.base.lock().unwrap();
        base.is_ai_position(position) || base.is_ai_takeover_position(position)
    }

    pub fn is_finished(&self) -> bool {
        !self.hands.is_empty() && self.hands.values().all(Vec::is_empty)
    }

    /// Classify the established lead combo of the current trick, if any.
    pub(crate) fn lead_combo(&self) -> Option<Combo> {
        let lead = self.current_trick.first()?;
        combo::classify(&lead.cards, &self.rules)
    }

    /// All legal follow plays for `position` against the given lead. The lead
    /// combo must already be established.
    pub(crate) fn legal_follows(&self, position: usize, lead: &Combo) -> Vec<Vec<i32>> {
        let Some(hand) = self.hands.get(&position) else {
            return Vec::new();
        };
        combo::enumerate_follows(hand, lead, &self.rules)
    }

    pub fn match_finished(&self) -> bool {
        self.next_target_rank().is_none()
    }

    pub fn next_position(&self, from: usize) -> Option<usize> {
        let positions = self.active_positions();
        let start = positions.iter().position(|position| *position == from)?;
        Some(positions[(start + 1) % positions.len()])
    }

    pub fn next_target_rank(&self) -> Option<TractorRank> {
        let winning_team = self.winner_positions_usize()[0] % 2;
        next_match_rank(
            self.team_target_ranks[winning_team],
            self.rules.final_target_rank,
            self.level_change() as usize,
        )
    }

    pub fn settlement_team_target_ranks(&self) -> Vec<TractorRank> {
        let mut target_ranks = self.team_target_ranks;
        if let Some(next_target_rank) = self.next_target_rank() {
            let winning_team = self.winner_positions_usize()[0] % 2;
            target_ranks[winning_team] = next_target_rank;
        }
        target_ranks.to_vec()
    }

    pub(crate) fn partner_still_to_play(&self, position: usize) -> bool {
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

    pub fn play_cards(
        &mut self,
        position: usize,
        name: String,
        mut cards: Vec<i32>,
    ) -> Result<WsTractorPlayedCards, &'static str> {
        if self.phase != TractorPhase::Play || self.current_position != position || cards.is_empty()
        {
            return Err("not current turn");
        }
        if cards.iter().any(|card| Card::try_from(*card).is_err()) {
            return Err("invalid card");
        }
        let hand = self.hands.get(&position).cloned().unwrap_or_default();
        let attempted_cards = cards.clone();
        let mut failed_throw = false;
        match self.lead_combo() {
            None => {
                // Leading: singles, pairs, tractors and same-group throws are
                // accepted. A challenged throw is reduced by the referee to its
                // weakest beatable component before any card leaves the hand.
                let Some(proposed) = combo::classify(&cards, &self.rules) else {
                    return Err("invalid play shape");
                };
                if !candidate_in_hand(&hand, &cards) {
                    return Err("card not in hand");
                }
                if matches!(proposed.kind, crate::combo::ComboKind::Throw { .. })
                    && let Some(fallback) = self.failed_throw_component(position, &cards)
                {
                    cards = fallback;
                    failed_throw = true;
                }
            }
            Some(lead) => {
                if !combo::follow_is_legal(&hand, &cards, &lead, &self.rules) {
                    return Err("illegal follow");
                }
            }
        }
        remove_cards_from_hand(self.hands.entry(position).or_default(), &cards)?;
        let played = WsTractorPlayedCards {
            position: position as i32,
            name,
            cards,
        };
        if failed_throw {
            self.failed_throws.push(TractorFailedThrow {
                position,
                attempted_cards,
                played_cards: played.cards.clone(),
                play_sequence: self.play_count,
            });
        }
        self.play_count += 1;
        self.current_trick.push(played.clone());
        if self.current_trick.len() >= self.active_positions().len() {
            let trick_score = combo::trick_points(&self.current_trick);
            let winner = combo::trick_winner(&self.current_trick, &self.rules).unwrap_or(position);
            let winning_cards = self
                .current_trick
                .iter()
                .find(|played| played.position == winner as i32)
                .map(|played| played.cards.clone())
                .unwrap_or_default();
            *self.collected_scores.entry(winner).or_default() += trick_score;
            self.last_trick_winner = Some(winner);
            self.bottom_multiplier = combo::bottom_multiplier(&winning_cards, &self.rules);
            self.completed_tricks.push(self.current_trick.clone());
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
            self.base.lock().unwrap().turn_countdown = 0;
            self.record_settlement_scores();
        }
        self.base.lock().unwrap().action_received = true;
        Ok(played)
    }

    pub fn last_failed_throw_event(&self, position: usize) -> Option<WsTractorFailedThrowEvent> {
        let play_sequence = self.play_count.checked_sub(1)?;
        self.failed_throws
            .iter()
            .rev()
            .find(|record| record.position == position && record.play_sequence == play_sequence)
            .map(|record| WsTractorFailedThrowEvent {
                position: record.position as i32,
                attempted_cards: record.attempted_cards.clone(),
                played_cards: record.played_cards.clone(),
            })
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn preferred_dealer_trump_suit(&self) -> TractorSuit {
        crate::ai::best_trump_suit(self, self.dealer_position)
    }

    pub fn remaining_hand_count(&self, position: usize) -> i32 {
        self.hands
            .get(&position)
            .map(|cards| cards.len() as i32)
            .unwrap_or_default()
    }

    pub fn select_dealer_trump(
        &mut self,
        position: usize,
        suit: TractorSuit,
    ) -> Result<WsTractorTrumpDeclaration, &'static str> {
        if self.round_index == 0 || self.phase != TractorPhase::Bury {
            return Err("dealer selects trump only in a later-round bottom operation");
        }
        if self.rules.trump_suit.is_some() {
            return Err("dealer trump is already selected");
        }
        if position != self.dealer_position {
            return Err("only dealer selects trump");
        }
        let declaration = WsTractorTrumpDeclaration {
            position: position as i32,
            name: self.player_name(position),
            cards: Vec::new(),
            trump_suit: suit,
            strength: 0,
            target_rank: self.rules.target_rank,
        };
        self.rules.trump_suit = Some(suit);
        self.declaration = Some(declaration.clone());
        Ok(declaration)
    }

    pub fn set_turn_countdown(&mut self, countdown: u32) {
        self.base.lock().unwrap().turn_countdown = countdown;
    }

    pub fn settlement_score(&self) -> i32 {
        self.attacking_score()
    }

    pub fn score_outcome(&self) -> ScoreOutcome {
        self.rules
            .score_progression()
            .outcome(self.attacking_score())
    }

    pub fn level_change(&self) -> i32 {
        i32::from(self.score_outcome().levels)
    }

    fn record_settlement_scores(&mut self) {
        let score = self.settlement_score();
        let winners = self.winner_positions_usize();
        for position in self.active_positions() {
            let delta = if winners.contains(&position) {
                score
            } else {
                -score
            };
            *self.player_scores.entry(position).or_default() += delta;
        }
    }

    pub fn player_scores_snapshot(&self) -> HashMap<i32, i32> {
        self.active_positions()
            .into_iter()
            .map(|position| {
                (
                    position as i32,
                    self.player_scores
                        .get(&position)
                        .copied()
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    pub fn snapshot(&self) -> WsTractorTableSnapshotEvent {
        let mut player_hand_counts: Vec<_> = self
            .hands
            .iter()
            .map(|(position, cards)| WsTractorPlayerHandCount {
                position: *position as i32,
                hand_count: cards.len() as i32,
            })
            .collect();
        player_hand_counts.sort_by_key(|player| player.position);
        let turn_countdown = self.base.lock().unwrap().turn_countdown as i32;
        let player_scores = self.player_scores_snapshot();
        let failed_throws = self
            .failed_throws
            .iter()
            .map(|record| WsTractorFailedThrowEvent {
                position: record.position as i32,
                attempted_cards: record.attempted_cards.clone(),
                played_cards: record.played_cards.clone(),
            })
            .collect();
        WsTractorTableSnapshotEvent {
            phase: self.phase,
            deck_count: self.rules.deck_count as i32,
            target_rank: self.rules.target_rank,
            team_target_ranks: self.team_target_ranks.to_vec(),
            final_target_rank: self.rules.final_target_rank,
            removed_rank_count: 0,
            round_index: self.round_index,
            attacking_win_score: self.rules.attacking_win_score,
            score_per_level: self.rules.score_per_level,
            shutout_bonus_levels: i32::from(self.rules.shutout_bonus_levels),
            bottom_card_count: self.bottom_cards.len() as i32,
            hand_count: self.hand_count() as i32,
            dealer_position: self.dealer_position as i32,
            trump_suit: self.rules.trump_suit,
            declaration: self.declaration.clone(),
            dealt_count: self.dealt_count as i32,
            total_deal_count: self.total_deal_count as i32,
            player_hand_counts,
            current_position: self.current_position as i32,
            trick_index: self.trick_index,
            current_trick: self.current_trick.clone(),
            turn_countdown,
            player_scores,
            failed_throws,
        }
    }

    pub fn winner_positions(&self) -> Vec<i32> {
        self.winner_positions_usize()
            .iter()
            .map(|position| *position as i32)
            .collect()
    }

    pub fn winner_positions_usize(&self) -> Vec<usize> {
        let attacking_score = self.attacking_score();
        let winners = if attacking_score >= self.rules.attacking_win_score {
            team_positions((self.dealer_position + 1) % 4)
        } else {
            team_positions(self.dealer_position)
        };
        winners.to_vec()
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
    pub fn score_progression(&self) -> ScoreProgression {
        ScoreProgression::new(
            self.attacking_win_score.max(1) as u32,
            self.score_per_level.max(1) as u32,
            self.shutout_bonus_levels,
        )
        .expect("normalized tractor score progression is valid")
    }
}

#[cfg(test)]
#[path = "game_state/coverage_tests.rs"]
mod coverage_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_bottom_counts_keep_all_hands_equal() {
        assert_eq!(standard_bottom_card_count(2), 8);
        assert_eq!(standard_bottom_card_count(3), 6);
        for deck_count in MIN_TRACTOR_DECK_COUNT..=MAX_TRACTOR_DECK_COUNT {
            let total = build_tractor_deck(deck_count).len();
            assert_eq!((total - standard_bottom_card_count(deck_count)) % 4, 0);
        }
    }

    #[test]
    fn away_following_opponent_prefers_smallest_winning_card() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.current_position = 1;
        state.current_trick.push(WsTractorPlayedCards {
            position: 0,
            name: "u0".to_owned(),
            cards: vec![4],
        });
        state.hands.insert(1, vec![5, 6, 13]);

        assert_eq!(state.choose_away_play(1), Some(vec![5]));
    }

    #[test]
    fn away_lead_uses_highest_non_scoring_side_card_as_a_single() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.rules.trump_suit = Some(TractorSuit::SPADE);
        state.current_position = 0;
        state.hands.insert(0, vec![13, 17, 22, 24, 25, 26]);

        assert_eq!(state.choose_away_play(0), Some(vec![26]));

        state.hands.insert(0, vec![13, 17, 22, 23, 24, 25]);
        assert_eq!(state.choose_away_play(0), Some(vec![24]));
    }

    #[test]
    fn away_feeds_points_whenever_partner_is_winning() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.current_position = 2;
        state.current_trick = vec![
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![13],
            },
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![3],
            },
        ];
        state.hands.insert(2, vec![3, 4, 9, 12]);

        assert_eq!(state.choose_away_play(2), Some(vec![9]));
    }

    #[test]
    fn away_ruffs_when_that_is_the_only_way_to_beat_an_opponent() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.rules.trump_suit = Some(TractorSuit::HEART);
        state.current_position = 1;
        state.current_trick = vec![WsTractorPlayedCards {
            position: 0,
            name: "u0".to_owned(),
            cards: vec![8],
        }];
        state.hands.insert(1, vec![15, 41]);

        assert_eq!(state.choose_away_play(1), Some(vec![15]));
    }

    #[test]
    fn away_avoids_points_when_an_opponent_cannot_be_beaten() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.current_position = 1;
        state.current_trick = vec![WsTractorPlayedCards {
            position: 0,
            name: "u0".to_owned(),
            cards: vec![13],
        }];
        state.hands.insert(1, vec![4, 9, 11, 12]);

        assert_eq!(state.choose_away_play(1), Some(vec![11]));
    }

    #[test]
    fn ai_only_feeds_points_to_a_winning_partner_when_last() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.current_position = 2;
        state.current_trick = vec![
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![11],
            },
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![3],
            },
        ];
        state.hands.insert(2, vec![2, 9]);
        assert_eq!(state.choose_auto_play(2), Some(vec![2]));

        state.current_position = 3;
        state.current_trick = vec![
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![3],
            },
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![11],
            },
            WsTractorPlayedCards {
                position: 2,
                name: "u2".to_owned(),
                cards: vec![4],
            },
        ];
        state.hands.insert(3, vec![2, 9]);
        assert_eq!(state.choose_auto_play(3), Some(vec![9]));
    }

    #[test]
    fn score_progression_reports_both_sides_and_high_score_levels() {
        let rules = TractorRules {
            attacking_win_score: 80,
            score_per_level: 40,
            shutout_bonus_levels: 1,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            target_rank: TractorRank::A,
            trump_suit: None,
        };
        assert_eq!(
            rules.score_progression().outcome(0),
            ScoreOutcome::defending(3)
        );
        assert_eq!(
            rules.score_progression().outcome(79),
            ScoreOutcome::defending(1)
        );
        assert_eq!(
            rules.score_progression().outcome(80),
            ScoreOutcome::attacking(1)
        );
        assert_eq!(
            rules.score_progression().outcome(120),
            ScoreOutcome::attacking(2)
        );
    }

    #[test]
    fn bottom_multiplier_tracks_last_winning_play_size() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.bottom_cards = vec![9]; // one 10-point card in the bottom
        // Single final trick: standard multiplier 2.
        state.hands.insert(0, vec![5]);
        state.hands.insert(1, vec![6]);
        state.hands.insert(2, vec![7]);
        state.hands.insert(3, vec![8]);
        for (pos, card) in [(0, 5), (1, 6), (2, 7), (3, 8)] {
            state
                .play_cards(pos, format!("u{pos}"), vec![card])
                .unwrap();
        }
        assert_eq!(state.bottom_multiplier, 2);
        // The winner (highest suit-0 single = position 3) banks bottom × 2.
        assert_eq!(state.last_trick_winner, Some(3));
        assert_eq!(state.collected_scores.get(&3).copied(), Some(20));

        // Now a pair-winning final trick: standard multiplier 4.
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.bottom_cards = vec![9];
        state.hands.insert(0, vec![5, 105]);
        state.hands.insert(1, vec![6, 106]);
        state.hands.insert(2, vec![7, 107]);
        state.hands.insert(3, vec![8, 108]);
        for (pos, cards) in [
            (0, vec![5, 105]),
            (1, vec![6, 106]),
            (2, vec![7, 107]),
            (3, vec![8, 108]),
        ] {
            state.play_cards(pos, format!("u{pos}"), cards).unwrap();
        }
        assert_eq!(state.bottom_multiplier, 4);
        // Winner banks bottom (10) × 4 = 40.
        assert_eq!(state.last_trick_winner, Some(3));
        assert_eq!(state.collected_scores.get(&3).copied(), Some(40));
    }

    #[test]
    fn failed_ace_queen_pair_throw_forces_out_queen_pair() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.hands.insert(0, vec![13, 113, 11, 111]);
        state.hands.insert(1, vec![12, 112, 20, 21]);
        state.hands.insert(2, vec![30, 31, 32, 33]);
        state.hands.insert(3, vec![42, 43, 44, 45]);

        let played = state
            .play_cards(0, "u0".to_owned(), vec![13, 113, 11, 111])
            .expect("throw is resolved by referee");

        assert_eq!(played.cards, vec![11, 111]);
        assert_eq!(state.current_trick[0].cards, vec![11, 111]);
        assert_eq!(state.hands[&0], vec![13, 113]);
    }

    #[test]
    fn incremental_deal_gives_bottom_to_dealer_then_requires_equal_bury() {
        let mut state = test_state();
        let mut rules = state.rules.clone();
        rules.target_rank = TractorRank::TWO;
        state.deal_new_round(rules).expect("prepare round");
        while state.phase == TractorPhase::Deal {
            state.deal_next_card().expect("next card");
        }
        assert_eq!(state.phase, TractorPhase::Bury);
        let dealer_count = state.remaining_hand_count(state.dealer_position) as usize;
        assert_eq!(
            dealer_count,
            state.hand_count() + state.rules.bottom_card_count
        );
        let bottom = state.choose_auto_bury().expect("automatic bottom");
        assert_eq!(bottom.len(), state.rules.bottom_card_count);
        state
            .bury_bottom(state.dealer_position, bottom)
            .expect("bury exact count");
        assert_eq!(state.phase, TractorPhase::Play);
        assert!(
            state
                .hands
                .values()
                .all(|cards| cards.len() == state.hand_count())
        );
    }

    #[test]
    fn later_round_cannot_bury_before_trump_is_selected() {
        let mut state = test_state();
        state.phase = TractorPhase::Bury;
        state.round_index = 1;
        state.dealer_position = 0;
        state.rules.bottom_card_count = 2;
        state.rules.trump_suit = None;
        state.hands.insert(0, vec![2, 3, 4, 5]);

        assert!(state.bury_bottom(0, vec![2, 3]).is_err());
        state.rules.trump_suit = Some(TractorSuit::DIAMOND);
        state.bury_bottom(0, vec![2, 3]).expect("selected first");
    }

    #[test]
    #[cfg(feature = "official")]
    fn later_round_trump_is_selected_only_by_the_established_dealer() {
        let mut state = test_state();
        state.phase = TractorPhase::Bury;
        state.round_index = 1;
        state.dealer_position = 2;
        state.rules.target_rank = TractorRank::FIVE;
        state.hands.insert(0, vec![4]);
        state.hands.insert(2, vec![14, 15, 16, 114, 115]);

        assert!(state.declare_trump(0, vec![4]).is_err());
        assert!(state.select_dealer_trump(1, TractorSuit::SPADE).is_err());
        assert_eq!(state.preferred_dealer_trump_suit(), TractorSuit::HEART);

        let selection = state
            .select_dealer_trump(2, TractorSuit::CLUB)
            .expect("dealer chooses freely");
        assert!(selection.cards.is_empty());
        assert_eq!(selection.strength, 0);
        assert_eq!(selection.position, 2);
        assert_eq!(state.rules.trump_suit, Some(TractorSuit::CLUB));
    }

    #[test]
    fn partner_higher_pair_also_breaks_a_throw() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.hands.insert(0, vec![13, 113, 11, 111]);
        state.hands.insert(1, vec![20, 21, 22, 23]);
        // Position 2 is the thrower's partner, but table validation must still
        // notice that their K pair can beat the proposed Q component.
        state.hands.insert(2, vec![12, 112, 30, 31]);
        state.hands.insert(3, vec![42, 43, 44, 45]);

        let played = state
            .play_cards(0, "u0".to_owned(), vec![13, 113, 11, 111])
            .expect("throw is resolved by referee");

        assert_eq!(played.cards, vec![11, 111]);
    }

    #[test]
    fn play_rejects_wrong_card_count_and_must_follow_suit() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::THREE;
        state.hands.insert(0, vec![3, 103]);
        state.hands.insert(1, vec![4, 17, 104]);

        state
            .play_cards(0, "u0".to_owned(), vec![3, 103])
            .expect("lead pair");
        assert!(state.play_cards(1, "u1".to_owned(), vec![4]).is_err());
        assert!(state.play_cards(1, "u1".to_owned(), vec![4, 17]).is_err());
        state
            .play_cards(1, "u1".to_owned(), vec![4, 104])
            .expect("follow lead suit pair");
    }

    #[test]
    fn settlement_advances_rank_until_final_target() {
        let mut state = test_state();
        state.rules.final_target_rank = TractorRank::NINE;
        state.rules.target_rank = TractorRank::EIGHT;
        state.team_target_ranks = [TractorRank::EIGHT, TractorRank::THREE];
        state.phase = TractorPhase::Settlement;

        assert_eq!(state.next_target_rank(), Some(TractorRank::NINE));
        assert!(state.advance_after_settlement().expect("advance"));
        assert_eq!(state.rules.target_rank, TractorRank::NINE);
        assert_eq!(
            state.team_target_ranks,
            [TractorRank::NINE, TractorRank::THREE]
        );

        state.phase = TractorPhase::Settlement;
        assert!(state.match_finished());
        assert_eq!(state.next_target_rank(), None);
        assert!(!state.advance_after_settlement().expect("finished"));
    }

    #[test]
    fn high_attacking_score_advances_multiple_ranks() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::SEVEN;
        state.team_target_ranks = [TractorRank::SEVEN, TractorRank::THREE];
        state.rules.final_target_rank = TractorRank::A;
        state.phase = TractorPhase::Settlement;
        state.collected_scores = HashMap::from([(1, 120)]);

        assert_eq!(state.score_outcome(), ScoreOutcome::attacking(2));
        assert_eq!(state.next_target_rank(), Some(TractorRank::FIVE));
        assert!(state.advance_after_settlement().expect("advance two ranks"));
        assert_eq!(state.rules.target_rank, TractorRank::FIVE);
        assert_eq!(state.dealer_position, 1);
        assert_eq!(
            state.team_target_ranks,
            [TractorRank::SEVEN, TractorRank::FIVE]
        );
    }

    #[test]
    fn losing_an_ace_round_does_not_finish_the_other_teams_match() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::A;
        state.rules.final_target_rank = TractorRank::A;
        state.team_target_ranks = [TractorRank::A, TractorRank::THREE];
        state.phase = TractorPhase::Settlement;
        state.collected_scores = HashMap::from([(1, 80)]);

        assert!(!state.match_finished());
        assert_eq!(state.next_target_rank(), Some(TractorRank::FOUR));
        assert_eq!(
            state.settlement_team_target_ranks(),
            vec![TractorRank::A, TractorRank::FOUR]
        );
    }

    #[test]
    #[cfg(feature = "official")]
    fn strong_ai_pair_can_counter_a_human_single_after_hand_evaluation() {
        let mut state = test_state();
        state.phase = TractorPhase::Deal;
        state.round_index = 0;
        state.rules.target_rank = TractorRank::TWO;
        state.hands.insert(0, vec![14]);
        state.hands.insert(1, vec![1, 101]);
        state.deal_queue.push_back((2, 3));
        state.base.lock().unwrap().mark_ai_position(1);

        state
            .declare_trump(0, vec![14])
            .expect("human declares a single heart 2");
        let (_, _, finished, auto_declaration) =
            state.deal_next_card().expect("deal the final card");

        assert!(finished);
        assert_eq!(auto_declaration.as_ref().map(|item| item.strength), Some(2));
        assert_eq!(
            state.declaration.as_ref().map(|item| item.position),
            Some(1)
        );
        assert_eq!(state.dealer_position, 1);
        assert_eq!(state.rules.trump_suit, Some(TractorSuit::SPADE));
        assert_eq!(state.phase, TractorPhase::Bury);
    }

    #[test]
    #[cfg(feature = "official")]
    fn ai_can_declare_before_first_deal_finishes() {
        let mut state = test_state();
        state.phase = TractorPhase::Deal;
        state.round_index = 0;
        state.rules.target_rank = TractorRank::TWO;
        state.hands.insert(1, vec![1, 101]);
        state.deal_queue.push_back((2, 3));
        state.deal_queue.push_back((3, 4));
        state.total_deal_count = 2;
        state.base.lock().unwrap().mark_ai_position(1);

        let (_, _, finished, auto_declaration) =
            state.deal_next_card().expect("deal the first card");

        assert!(!finished);
        assert_eq!(auto_declaration.as_ref().map(|item| item.strength), Some(2));
        assert_eq!(
            state.declaration.as_ref().map(|item| item.position),
            Some(1)
        );
        assert_eq!(state.rules.trump_suit, Some(TractorSuit::SPADE));
        assert_eq!(state.phase, TractorPhase::Deal);
    }

    #[test]
    fn stronger_level_card_declaration_sets_first_dealer_and_trump_suit() {
        let mut state = test_state();
        state.phase = TractorPhase::Deal;
        state.round_index = 0;
        state.rules.target_rank = TractorRank::THREE;
        state.hands.insert(1, vec![2, 102]);
        state.hands.insert(2, vec![15]);

        let first = state.declare_trump(2, vec![15]).expect("single heart 3");
        assert_eq!(first.trump_suit, TractorSuit::HEART);
        assert_eq!(state.dealer_position, 2);
        assert!(state.declare_trump(1, vec![2]).is_err());

        let counter = state
            .declare_trump(1, vec![2, 102])
            .expect("pair of spade 3 counters single");
        assert_eq!(counter.strength, 2);
        assert_eq!(state.rules.trump_suit, Some(TractorSuit::SPADE));
        assert_eq!(state.dealer_position, 1);
    }

    fn test_state() -> TractorGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state = TractorGameState::from_common(Arc::new(Mutex::new(common)));
        state.phase = TractorPhase::Play;
        state.rules = TractorRules {
            attacking_win_score: 80,
            score_per_level: 40,
            shutout_bonus_levels: 1,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            target_rank: TractorRank::A,
            trump_suit: None,
        };
        state.dealer_position = 0;
        state.current_position = 0;
        state
    }

    #[test]
    fn three_decks_uses_six_bottom_cards_and_thirty_nine_card_hands() {
        let total = build_tractor_deck(3).len();
        let bottom = standard_bottom_card_count(3);
        assert_eq!(bottom, 6);
        assert_eq!((total - bottom) % 4, 0);
        assert_eq!((total - bottom) / 4, 39);
    }

    #[test]
    fn tractor_lead_forces_pair_follow_and_higher_tractor_wins() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        // Lead a suit-0 tractor rank3+rank4; each opponent must follow shape.
        state.hands.insert(0, vec![2, 102, 3, 103]);
        state.hands.insert(1, vec![5, 105, 6, 106]); // higher suit-0 tractor
        state.hands.insert(2, vec![18, 118, 19, 119]);
        state.hands.insert(3, vec![31, 131, 32, 132]);

        state
            .play_cards(0, "u0".to_owned(), vec![2, 102, 3, 103])
            .expect("lead tractor");
        // A single pair cannot answer a tractor lead (wrong card count).
        assert!(state.play_cards(1, "u1".to_owned(), vec![5, 105]).is_err());
        state
            .play_cards(1, "u1".to_owned(), vec![5, 105, 6, 106])
            .expect("follow higher tractor");
        state
            .play_cards(2, "u2".to_owned(), vec![18, 118, 19, 119])
            .unwrap();
        state
            .play_cards(3, "u3".to_owned(), vec![31, 131, 32, 132])
            .unwrap();

        // Position 1's higher suit-0 tractor takes the trick.
        assert_eq!(state.last_trick_winner, Some(1));
    }

    #[test]
    fn snapshot_exposes_failed_throw_history() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.hands.insert(0, vec![13, 113, 11, 111]);
        state.hands.insert(1, vec![12, 112, 20, 21]);
        state.hands.insert(2, vec![30, 31, 32, 33]);
        state.hands.insert(3, vec![42, 43, 44, 45]);

        state
            .play_cards(0, "u0".to_owned(), vec![13, 113, 11, 111])
            .expect("failed throw is accepted and reduced");

        let snapshot = state.snapshot();
        assert_eq!(snapshot.failed_throws.len(), 1);
        assert_eq!(snapshot.failed_throws[0].position, 0);
        assert_eq!(
            snapshot.failed_throws[0].attempted_cards,
            vec![13, 113, 11, 111]
        );
        assert_eq!(snapshot.failed_throws[0].played_cards, vec![11, 111]);
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
        assert_eq!(state.completed_tricks.len(), 1);
        assert_eq!(state.completed_tricks[0].len(), 4);
        assert!(state.current_trick.is_empty());
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
        assert_eq!(state.attacking_score(), 155);
        assert_eq!(state.winner_positions(), vec![1, 3]);
        assert_eq!(state.settlement_score(), 155);
        assert_eq!(state.player_scores.get(&0), Some(&-155));
        assert_eq!(state.player_scores.get(&1), Some(&155));
        assert_eq!(state.player_scores.get(&2), Some(&-155));
        assert_eq!(state.player_scores.get(&3), Some(&155));
        assert_eq!(
            state.snapshot().player_scores,
            state.player_scores_snapshot()
        );
    }

    #[test]
    fn first_deal_falls_back_to_an_available_level_card() {
        let mut state = test_state();
        state.phase = TractorPhase::Deal;
        state.round_index = 0;
        state.rules.target_rank = TractorRank::TWO;
        state.hands.insert(1, vec![1]);
        state.deal_queue.push_back((2, 3));
        state.base.lock().unwrap().mark_ai_position(1);

        let (_, _, finished, auto_declaration) =
            state.deal_next_card().expect("deal the final card");

        assert!(finished);
        let declaration = auto_declaration.expect("fallback declaration");
        assert_eq!(declaration.position, 1);
        assert_eq!(declaration.cards, vec![1]);
        assert_eq!(
            state.declaration.as_ref().map(|item| item.position),
            Some(1)
        );
        assert_eq!(
            state.declaration.as_ref().map(|item| item.cards.as_slice()),
            Some([1].as_slice())
        );
        assert_eq!(state.dealer_position, 1);
        assert_eq!(state.rules.trump_suit, Some(TractorSuit::SPADE));
        assert_eq!(state.phase, TractorPhase::Bury);
    }

    #[test]
    fn first_deal_can_reveal_a_level_card_from_the_bottom() {
        let mut state = test_state();
        state.phase = TractorPhase::Deal;
        state.round_index = 0;
        state.rules.target_rank = TractorRank::THREE;
        state.bottom_cards = vec![2];
        state.deal_queue.push_back((2, 4));

        let (_, _, finished, declaration) = state.deal_next_card().expect("deal the final card");

        assert!(finished);
        let declaration = declaration.expect("bottom fallback declaration");
        assert_eq!(declaration.position, 0);
        assert_eq!(declaration.cards, vec![2]);
        assert_eq!(state.rules.trump_suit, Some(TractorSuit::SPADE));
        assert_eq!(state.phase, TractorPhase::Bury);
    }
}
