use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use rand::seq::SliceRandom;
use share_type_public::TexasHoldEmPhase;
use ws_common::game_state::{CommonGameState, GameState};

use crate::hand_evaluator::{EvaluatedHand, evaluate_best};

pub type TexasHoldEmStateHandle = Arc<Mutex<TexasHoldEmGameState>>;

#[derive(Debug)]
pub struct TexasHoldEmGameState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: TexasHoldEmPhase,
    pub deck: Vec<i32>,
    pub public_cards: Vec<i32>,
    pub hands: HashMap<usize, Vec<i32>>,
    pub chips: HashMap<usize, i32>,
    pub round_bets: HashMap<usize, i32>,
    pub folded: HashSet<usize>,
    pub all_in: HashSet<usize>,
    pub acted: HashSet<usize>,
    pub dealer_position: usize,
    pub small_blind_position: usize,
    pub big_blind_position: usize,
    pub current_position: usize,
    pub current_bet: i32,
    pub min_raise: i32,
    pub pot: i32,
    pub initial_chips: i32,
    pub small_blind: i32,
    pub big_blind: i32,
}

impl TexasHoldEmGameState {
    pub fn from_common(base: Arc<Mutex<CommonGameState>>) -> Self {
        Self {
            base,
            phase: TexasHoldEmPhase::Start,
            deck: Vec::new(),
            public_cards: Vec::new(),
            hands: HashMap::new(),
            chips: HashMap::new(),
            round_bets: HashMap::new(),
            folded: HashSet::new(),
            all_in: HashSet::new(),
            acted: HashSet::new(),
            dealer_position: 0,
            small_blind_position: 0,
            big_blind_position: 0,
            current_position: 0,
            current_bet: 0,
            min_raise: 0,
            pot: 0,
            initial_chips: 1000,
            small_blind: 5,
            big_blind: 10,
        }
    }

    pub fn active_positions(&self) -> Vec<usize> {
        let mut positions: Vec<_> = self.base.lock().unwrap().players.keys().copied().collect();
        positions.sort_unstable();
        positions
    }

    pub fn active_not_folded_positions(&self) -> Vec<usize> {
        self.active_positions()
            .into_iter()
            .filter(|position| !self.folded.contains(position))
            .collect()
    }

    pub fn bet_of(&self, position: usize) -> i32 {
        self.round_bets.get(&position).copied().unwrap_or_default()
    }

    pub fn call_amount(&self, position: usize) -> i32 {
        (self.current_bet - self.bet_of(position)).max(0)
    }

    pub fn chip_count(&self, position: usize) -> i32 {
        self.chips.get(&position).copied().unwrap_or_default()
    }

    pub fn deal_new_hand(
        &mut self,
        initial_chips: i32,
        small_blind: i32,
        big_blind: i32,
    ) -> Result<(), &'static str> {
        let positions = self.active_positions();
        if !(2..=8).contains(&positions.len()) {
            return Err("Texas Hold'em requires 2-8 players");
        }
        self.phase = TexasHoldEmPhase::PreFlop;
        self.deck = (1..=52).collect();
        self.deck.shuffle(&mut rand::rng());
        self.public_cards.clear();
        self.hands.clear();
        self.chips.clear();
        self.round_bets.clear();
        self.folded.clear();
        self.all_in.clear();
        self.acted.clear();
        self.pot = 0;
        self.current_bet = big_blind;
        self.min_raise = big_blind;
        self.initial_chips = initial_chips;
        self.small_blind = small_blind;
        self.big_blind = big_blind;
        self.dealer_position = positions[0];
        self.small_blind_position = self
            .next_position(self.dealer_position)
            .unwrap_or(positions[0]);
        self.big_blind_position = self
            .next_position(self.small_blind_position)
            .unwrap_or(self.small_blind_position);

        for position in &positions {
            self.chips.insert(*position, initial_chips);
            self.round_bets.insert(*position, 0);
            let first = self.deck.pop().ok_or("deck exhausted")?;
            let second = self.deck.pop().ok_or("deck exhausted")?;
            self.hands.insert(*position, vec![first, second]);
        }

        self.commit(self.small_blind_position, small_blind);
        self.commit(self.big_blind_position, big_blind);
        self.current_position = self
            .next_action_position(self.big_blind_position)
            .unwrap_or(self.big_blind_position);
        Ok(())
    }

    pub fn commit(&mut self, position: usize, amount: i32) -> i32 {
        let available = self.chip_count(position);
        let paid = amount.max(0).min(available);
        if paid == 0 {
            return 0;
        }
        *self.chips.entry(position).or_default() -= paid;
        *self.round_bets.entry(position).or_default() += paid;
        self.pot += paid;
        if self.chip_count(position) == 0 {
            self.all_in.insert(position);
        }
        paid
    }

    pub fn is_hand_over_by_folds(&self) -> bool {
        self.active_not_folded_positions().len() <= 1
    }

    pub fn is_round_complete(&self) -> bool {
        self.active_positions().into_iter().all(|position| {
            if self.folded.contains(&position) || self.all_in.contains(&position) {
                return true;
            }
            self.acted.contains(&position) && self.bet_of(position) == self.current_bet
        })
    }

    pub fn next_action_position(&self, from: usize) -> Option<usize> {
        let positions = self.active_positions();
        let start = positions.iter().position(|position| *position == from)?;
        for offset in 1..=positions.len() {
            let candidate = positions[(start + offset) % positions.len()];
            if self.folded.contains(&candidate) || self.all_in.contains(&candidate) {
                continue;
            }
            return Some(candidate);
        }
        None
    }

    pub fn next_position(&self, from: usize) -> Option<usize> {
        let positions = self.active_positions();
        let start = positions.iter().position(|position| *position == from)?;
        Some(positions[(start + 1) % positions.len()])
    }

    pub fn reveal_next_phase(&mut self) -> TexasHoldEmPhase {
        self.round_bets.clear();
        self.acted.clear();
        self.current_bet = 0;
        self.min_raise = self.big_blind;
        self.phase = match self.phase {
            TexasHoldEmPhase::PreFlop => {
                for _ in 0..3 {
                    if let Some(card) = self.deck.pop() {
                        self.public_cards.push(card);
                    }
                }
                TexasHoldEmPhase::Flop
            }
            TexasHoldEmPhase::Flop => {
                if let Some(card) = self.deck.pop() {
                    self.public_cards.push(card);
                }
                TexasHoldEmPhase::Turn
            }
            TexasHoldEmPhase::Turn => {
                if let Some(card) = self.deck.pop() {
                    self.public_cards.push(card);
                }
                TexasHoldEmPhase::River
            }
            TexasHoldEmPhase::River => TexasHoldEmPhase::Settlement,
            other => other,
        };
        if self.phase != TexasHoldEmPhase::Settlement {
            self.current_position = self
                .next_action_position(self.dealer_position)
                .unwrap_or(self.dealer_position);
        }
        self.phase
    }

    pub fn set_action_received(&mut self, received: bool) {
        self.base.lock().unwrap().action_received = received;
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn evaluated_hand(&self, position: usize) -> Option<EvaluatedHand> {
        let mut cards = self.hands.get(&position)?.clone();
        cards.extend(self.public_cards.iter().copied());
        evaluate_best(&cards)
    }
}

impl GameState for TexasHoldEmGameState {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.base)
    }
}
