use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use rand::seq::SliceRandom;
use share_type_public::{
    TractorPhase, TractorRank, TractorSuit, WsTractorPlayedCards, WsTractorPlayerHandCount,
    WsTractorTableSnapshotEvent, WsTractorTrumpDeclaration,
};
use ws_common::{CommonGameState, GameState};

use crate::combo::{self, Combo};

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

/// Ranks removed by the room's compact-deck setting, in order. Scoring ranks
/// (5, 10 and K), 2 and jokers are deliberately retained. Thus 3 removes
/// 3/4/6 and 4 removes 3/4/6/7, matching the room setting shown to users.
pub const REMOVABLE_RANKS: [TractorRank; 9] = [
    TractorRank::THREE,
    TractorRank::FOUR,
    TractorRank::SIX,
    TractorRank::SEVEN,
    TractorRank::EIGHT,
    TractorRank::NINE,
    TractorRank::J,
    TractorRank::Q,
    TractorRank::A,
];

#[derive(Debug)]
pub struct TractorGameState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: TractorPhase,
    pub rules: TractorRules,
    pub hands: HashMap<usize, Vec<i32>>,
    pub deal_queue: VecDeque<(usize, i32)>,
    pub dealt_count: usize,
    pub total_deal_count: usize,
    pub bottom_cards: Vec<i32>,
    pub declaration: Option<WsTractorTrumpDeclaration>,
    pub bottom_multiplier: i32,
    pub collected_scores: HashMap<usize, i32>,
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
}

#[derive(Debug, Clone)]
pub struct TractorRules {
    pub blood_enabled: bool,
    pub blood_score_per_unit: i32,
    pub blood_start_score: i32,
    pub bottom_card_count: usize,
    pub deck_count: usize,
    pub final_target_rank: TractorRank,
    pub removed_rank_count: usize,
    pub target_rank: TractorRank,
    pub trump_suit: Option<TractorSuit>,
}

pub type TractorStateHandle = Arc<Mutex<TractorGameState>>;

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
        .find(|bottom| (total_cards - bottom).is_multiple_of(player_count))
        .or_else(|| {
            (minimum..preferred)
                .rev()
                .find(|bottom| (total_cards - bottom).is_multiple_of(player_count))
        })
}

pub(crate) fn base_card(card: i32) -> i32 {
    ((card - 1) % 100) + 1
}

pub fn build_tractor_deck(deck_count: usize) -> Vec<i32> {
    build_tractor_deck_with_removed_ranks(deck_count, 0)
}

pub fn build_tractor_deck_with_removed_ranks(
    deck_count: usize,
    removed_rank_count: usize,
) -> Vec<i32> {
    let deck_count = deck_count.clamp(2, 4);
    let mut cards = Vec::with_capacity(deck_count * 54);
    for deck_index in 0..deck_count {
        let offset = deck_index as i32 * 100;
        for card in 1..=54 {
            let full_card = offset + card;
            let rank = card_rank(full_card);
            let should_remove = REMOVABLE_RANKS
                .iter()
                .take(removed_rank_count.min(REMOVABLE_RANKS.len()))
                .any(|item| *item as i32 == rank);
            if !should_remove {
                cards.push(full_card);
            }
        }
    }
    cards
}

pub(crate) fn card_rank(card: i32) -> i32 {
    let base = base_card(card);
    if base <= 52 {
        ((base - 1) % 13) + 2
    } else if base == 53 {
        16
    } else {
        17
    }
}

pub(crate) fn card_score(card: i32) -> i32 {
    match card_rank(card) {
        5 => 5,
        10 | 13 => 10,
        _ => 0,
    }
}

pub(crate) fn card_suit(card: i32) -> Option<i32> {
    let base = base_card(card);
    (base <= 52).then_some((base - 1) / 13)
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

fn first_match_rank(removed_rank_count: usize, final_target_rank: TractorRank) -> TractorRank {
    tractor_rank_path(removed_rank_count, final_target_rank)
        .first()
        .copied()
        .unwrap_or(final_target_rank)
}

pub(crate) fn is_trump_card(card: i32, rules: &TractorRules) -> bool {
    card_suit(card).is_none()
        || card_rank(card) == rules.target_rank as i32
        || rules
            .trump_suit
            .is_some_and(|suit| card_suit(card) == Some(suit as i32))
}

pub fn min_bottom_card_count(deck_count: usize) -> usize {
    match deck_count {
        3 => 10,
        2 | 4 => 8,
        _ => 8,
    }
}

fn next_match_rank(
    current_rank: TractorRank,
    removed_rank_count: usize,
    final_target_rank: TractorRank,
) -> Option<TractorRank> {
    let path = tractor_rank_path(removed_rank_count, final_target_rank);
    let index = path.iter().position(|rank| *rank == current_rank)?;
    path.get(index + 1).copied()
}

fn played_score(cards: &[i32]) -> i32 {
    cards.iter().map(|card| card_score(*card)).sum()
}

fn rank_is_removed(removed_rank_count: usize, rank: TractorRank) -> bool {
    REMOVABLE_RANKS
        .iter()
        .take(removed_rank_count.min(REMOVABLE_RANKS.len()))
        .any(|item| *item == rank)
}

fn candidate_in_hand(hand: &[i32], cards: &[i32]) -> bool {
    let mut available = hand.to_vec();
    remove_cards_from_hand(&mut available, cards).is_ok()
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

fn team_positions(position: usize) -> [usize; 2] {
    [position, (position + 2) % 4]
}

/// Two seats are partners when they sit across from each other (0&2, 1&3).
pub(crate) fn same_team(a: usize, b: usize) -> bool {
    a % 2 == b % 2
}

pub(crate) fn tractor_card_value(card: i32, rules: &TractorRules, lead_suit: Option<i32>) -> i32 {
    let rank = card_rank(card);
    if is_trump_card(card, rules) {
        let suit = card_suit(card);
        if suit.is_none() {
            return 1_200 + rank;
        }
        if rank == rules.target_rank as i32 {
            return if rules
                .trump_suit
                .is_some_and(|trump| suit == Some(trump as i32))
            {
                1_100
            } else {
                1_000
            };
        }
        return 900 + rank;
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

pub fn removed_tractor_ranks(removed_rank_count: usize) -> Vec<TractorRank> {
    REMOVABLE_RANKS
        .iter()
        .take(removed_rank_count.min(REMOVABLE_RANKS.len()))
        .copied()
        .collect()
}

pub fn tractor_rank_path(
    removed_rank_count: usize,
    final_target_rank: TractorRank,
) -> Vec<TractorRank> {
    let mut out = Vec::new();
    for rank in TRACTOR_RANKS {
        if rank as i32 > final_target_rank as i32 {
            break;
        }
        if !rank_is_removed(removed_rank_count, rank) {
            out.push(rank);
        }
    }
    if out.is_empty() {
        out.push(TractorRank::TWO);
    }
    out
}

impl TractorGameState {
    pub fn active_positions(&self) -> Vec<usize> {
        let mut positions: Vec<_> = self.base.lock().unwrap().players.keys().copied().collect();
        positions.sort_unstable();
        positions
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

    /// Classify the established lead combo of the current trick, if any.
    pub(crate) fn lead_combo(&self) -> Option<Combo> {
        let lead = self.current_trick.first()?;
        combo::classify(&lead.cards, &self.rules)
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

    /// A safe, rules-correct auto play used for timed-out humans and as the AI
    /// fallback. Leads the lowest single; when following, beats an opponent with
    /// the smallest winning play, feeds points to a winning partner, and
    /// otherwise sheds the lowest legal cards.
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
                // Don't burn trump to ruff a pointless plain-suit trick when the
                // partner still has a turn to try to take it themselves.
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

        // Can't (or needn't) win: dump points to a winning partner, else shed low.
        candidates.sort_by_key(|cards| {
            if partner_winning && is_last_to_play {
                (-played_score(cards), strength(cards))
            } else {
                (played_score(cards), strength(cards))
            }
        });
        candidates.into_iter().next()
    }

    /// All legal follow plays for `position` against the given lead. The lead
    /// combo must already be established.
    pub(crate) fn legal_follows(&self, position: usize, lead: &Combo) -> Vec<Vec<i32>> {
        let Some(hand) = self.hands.get(&position) else {
            return Vec::new();
        };
        let mut out: Vec<Vec<i32>> = Vec::new();
        // The rules-correct minimum play is always legal.
        if let Some(base) = combo::forced_follow(hand, lead, &self.rules) {
            out.push(base);
        }
        // Enrich with every same-shape combo the hand can form; keep only legal ones.
        for cards in combo::enumerate_leads(hand, &self.rules) {
            if combo::classify(&cards, &self.rules).map(|c| c.kind) == Some(lead.kind)
                && combo::follow_is_legal(hand, &cards, lead, &self.rules)
                && !out.contains(&cards)
            {
                out.push(cards);
            }
        }
        out
    }

    fn deal_current_round(&mut self) -> Result<(), &'static str> {
        let positions = self.active_positions();
        if positions.len() != 4 {
            return Err("Tractor requires exactly 4 players");
        }

        let mut deck = build_tractor_deck_with_removed_ranks(
            self.rules.deck_count,
            self.rules.removed_rank_count,
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

    /// Deal exactly one public-progress/private-card step. The final step moves
    /// the table into Bury and gives the dealer the bottom cards.
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
        if finished {
            if self.round_index == 0 {
                // AI is only the fallback when no human has declared. Waiting
                // until the final card preserves the full first-round counter
                // window and prevents an AI buzzer counter.
                let current_strength = self
                    .declaration
                    .as_ref()
                    .map(|declaration| declaration.strength)
                    .unwrap_or_default();
                let ai_positions: Vec<_> = self
                    .active_positions()
                    .into_iter()
                    .filter(|position| self.is_ai_controlled_position(*position))
                    .collect();
                let assessed = ai_positions
                    .iter()
                    .filter_map(|position| {
                        crate::ai::declaration_decision(self, *position, current_strength, false)
                            .map(|decision| (*position, decision))
                    })
                    .max_by(|(_, left), (_, right)| {
                        left.cards.len().cmp(&right.cards.len()).then_with(|| {
                            left.assessment
                                .success_probability
                                .total_cmp(&right.assessment.success_probability)
                        })
                    });
                // If everybody passed, pick the best AI candidate as a final
                // table fallback so a single level card is not an automatic
                // personal bid, while the round still always gets a dealer.
                let best_ai_declaration = assessed.or_else(|| {
                    self.declaration
                        .is_none()
                        .then(|| {
                            ai_positions
                                .iter()
                                .filter_map(|position| {
                                    crate::ai::declaration_decision(self, *position, 0, true)
                                        .map(|decision| (*position, decision))
                                })
                                .max_by(|(_, left), (_, right)| {
                                    left.assessment
                                        .success_probability
                                        .total_cmp(&right.assessment.success_probability)
                                        .then_with(|| {
                                            left.assessment.score.cmp(&right.assessment.score)
                                        })
                                })
                        })
                        .flatten()
                });
                if let Some((position, decision)) = best_ai_declaration {
                    let cards = decision.cards;
                    auto_declaration = self.declare_trump(position, cards).ok();
                }
                if let Some(declaration) = &self.declaration {
                    self.dealer_position = declaration.position as usize;
                }
            } else if self.declaration.is_none() {
                // From round two onward the established dealer owns the choice.
                // If a human has not selected during the deal window, choose the
                // dealer's longest/most paired natural suit at the buzzer.
                let suit = self.preferred_dealer_trump_suit();
                auto_declaration = self.select_dealer_trump(self.dealer_position, suit).ok();
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

    pub fn select_dealer_trump(
        &mut self,
        position: usize,
        suit: TractorSuit,
    ) -> Result<WsTractorTrumpDeclaration, &'static str> {
        if self.round_index == 0 || self.phase != TractorPhase::Deal {
            return Err("dealer selects trump only in later deal phases");
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

    pub fn preferred_dealer_trump_suit(&self) -> TractorSuit {
        crate::ai::best_trump_suit(self, self.dealer_position)
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

    pub fn dealer_bottom_cards(&self) -> Option<Vec<i32>> {
        (self.phase == TractorPhase::Bury).then(|| self.bottom_cards.clone())
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

    pub fn choose_auto_bury(&self) -> Option<Vec<i32>> {
        if self.phase != TractorPhase::Bury {
            return None;
        }
        crate::ai::choose_bury(self)
    }

    pub fn deal_new_round(&mut self, mut rules: TractorRules) -> Result<(), &'static str> {
        rules.deck_count = rules.deck_count.clamp(2, 4);
        rules.blood_score_per_unit = rules.blood_score_per_unit.max(1);
        let positions = self.active_positions();
        if positions.len() != 4 {
            return Err("Tractor requires exactly 4 players");
        }
        rules.removed_rank_count = rules.removed_rank_count.min(REMOVABLE_RANKS.len());
        rules.target_rank = first_match_rank(rules.removed_rank_count, rules.final_target_rank);
        rules.trump_suit = None;
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
                removed_rank_count: 0,
                target_rank: TractorRank::A,
                trump_suit: None,
            },
            hands: HashMap::new(),
            deal_queue: VecDeque::new(),
            dealt_count: 0,
            total_deal_count: 0,
            bottom_cards: Vec::new(),
            declaration: None,
            bottom_multiplier: 1,
            collected_scores: HashMap::new(),
            last_trick_winner: None,
            dealer_position: 0,
            current_position: 0,
            round_index: 0,
            trick_index: 0,
            completed_tricks: Vec::new(),
            current_trick: Vec::new(),
        }
    }

    pub fn hand_count(&self) -> usize {
        if self.total_deal_count > 0 {
            self.total_deal_count / self.active_positions().len().max(1)
        } else {
            self.hands.values().map(Vec::len).max().unwrap_or_default()
        }
    }

    pub fn is_ai_controlled_position(&self, position: usize) -> bool {
        let base = self.base.lock().unwrap();
        base.is_ai_position(position) || base.is_away(position) || base.is_disconnected(position)
    }

    pub fn is_finished(&self) -> bool {
        !self.hands.is_empty() && self.hands.values().all(Vec::is_empty)
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
        next_match_rank(
            self.rules.target_rank,
            self.rules.removed_rank_count,
            self.rules.final_target_rank,
        )
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
        cards: Vec<i32>,
    ) -> Result<WsTractorPlayedCards, &'static str> {
        if self.phase != TractorPhase::Play || self.current_position != position || cards.is_empty()
        {
            return Err("not current turn");
        }
        let hand = self.hands.get(&position).cloned().unwrap_or_default();
        match self.lead_combo() {
            None => {
                // Leading: must be a well-formed single / pair / tractor in hand.
                if combo::classify(&cards, &self.rules).is_none() {
                    return Err("invalid play shape");
                }
                if !candidate_in_hand(&hand, &cards) {
                    return Err("card not in hand");
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
        self.current_trick.push(played.clone());
        if self.current_trick.len() >= self.active_positions().len() {
            let trick_score = combo::trick_points(&self.current_trick);
            let winner = combo::trick_winner(&self.current_trick, &self.rules).unwrap_or(position);
            let winning_len = self
                .current_trick
                .iter()
                .find(|played| played.position == winner as i32)
                .map(|played| played.cards.len())
                .unwrap_or(1);
            *self.collected_scores.entry(winner).or_default() += trick_score;
            self.last_trick_winner = Some(winner);
            // Bottom (扣底) reward is multiplied by the size of the last winning play.
            self.bottom_multiplier = (winning_len as i32).max(1);
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
        let mut player_hand_counts: Vec<_> = self
            .hands
            .iter()
            .map(|(position, cards)| WsTractorPlayerHandCount {
                position: *position as i32,
                hand_count: cards.len() as i32,
            })
            .collect();
        player_hand_counts.sort_by_key(|player| player.position);
        WsTractorTableSnapshotEvent {
            phase: self.phase,
            deck_count: self.rules.deck_count as i32,
            target_rank: self.rules.target_rank,
            final_target_rank: self.rules.final_target_rank,
            removed_rank_count: self.rules.removed_rank_count as i32,
            round_index: self.round_index,
            blood_enabled: self.rules.blood_enabled,
            blood_start_score: self.rules.blood_start_score,
            blood_score_per_unit: self.rules.blood_score_per_unit,
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
            turn_countdown: self.base.lock().unwrap().turn_countdown as i32,
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
        let winners = if attacking_score >= self.rules.blood_start_score {
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
    fn compact_deck_count_removes_the_documented_non_scoring_ranks() {
        assert_eq!(build_tractor_deck_with_removed_ranks(2, 0).len(), 108);
        let ranks = |count| {
            build_tractor_deck_with_removed_ranks(2, count)
                .into_iter()
                .map(card_rank)
                .collect::<Vec<_>>()
        };
        let removed_three = ranks(3);
        assert!(!removed_three.iter().any(|rank| [3, 4, 6].contains(rank)));
        assert!(removed_three.contains(&7));
        assert!(removed_three.contains(&5));
        assert!(removed_three.contains(&10));
        assert!(removed_three.contains(&13));

        let removed_four = ranks(4);
        assert!(!removed_four.iter().any(|rank| [3, 4, 6, 7].contains(rank)));
        assert!(removed_four.contains(&8));
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

    #[test]
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
    fn later_round_trump_is_selected_only_by_the_established_dealer() {
        let mut state = test_state();
        state.phase = TractorPhase::Deal;
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
    fn blood_units_start_after_threshold() {
        let rules = TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            removed_rank_count: 0,
            target_rank: TractorRank::A,
            trump_suit: None,
        };
        assert_eq!(rules.blood_units(79), 0);
        assert_eq!(rules.blood_units(80), 1);
        assert_eq!(rules.blood_units(120), 2);
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

    #[test]
    fn rank_path_skips_removed_ranks_and_keeps_final_target() {
        assert_eq!(
            tractor_rank_path(4, TractorRank::NINE),
            vec![
                TractorRank::TWO,
                TractorRank::FIVE,
                TractorRank::EIGHT,
                TractorRank::NINE
            ]
        );
    }

    #[test]
    fn settlement_advances_rank_until_final_target() {
        let mut state = test_state();
        state.rules.final_target_rank = TractorRank::NINE;
        state.rules.removed_rank_count = 4;
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
            removed_rank_count: 0,
            target_rank: TractorRank::A,
            trump_suit: None,
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
        assert_eq!(state.attacking_score(), 80);
        assert_eq!(state.winner_positions(), vec![1, 3]);
        assert_eq!(state.settlement_score(), 80);
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
    fn bottom_multiplier_tracks_last_winning_play_size() {
        let mut state = test_state();
        state.rules.target_rank = TractorRank::TWO;
        state.bottom_cards = vec![9]; // one 10-point card in the bottom
        // Single final trick: multiplier 1.
        state.hands.insert(0, vec![5]);
        state.hands.insert(1, vec![6]);
        state.hands.insert(2, vec![7]);
        state.hands.insert(3, vec![8]);
        for (pos, card) in [(0, 5), (1, 6), (2, 7), (3, 8)] {
            state
                .play_cards(pos, format!("u{pos}"), vec![card])
                .unwrap();
        }
        assert_eq!(state.bottom_multiplier, 1);
        // The winner (highest suit-0 single = position 3) banks bottom × 1.
        assert_eq!(state.last_trick_winner, Some(3));
        assert_eq!(state.collected_scores.get(&3).copied(), Some(10));

        // Now a pair-winning final trick: multiplier 2.
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
        assert_eq!(state.bottom_multiplier, 2);
        // Winner banks bottom (10) × 2 = 20.
        assert_eq!(state.last_trick_winner, Some(3));
        assert_eq!(state.collected_scores.get(&3).copied(), Some(20));
    }
}
