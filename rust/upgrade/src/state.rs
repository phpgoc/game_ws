use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rand::seq::SliceRandom;
use share_type_public::{
    UpgradePhase, UpgradeRank, UpgradeSuit, WsUpgradePlayerHandCount, WsUpgradeTableSnapshotEvent,
    WsUpgradeTrumpDeclaration,
};
use upgrade_common::{Card, Rank, Suit};
use ws_common::{CommonGameState, GameState};

use crate::UpgradeDeckCount;

pub const PLAYER_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeRules {
    pub deck_count: UpgradeDeckCount,
    pub target_rank: Rank,
    pub final_target_rank: Rank,
    pub attacking_win_score: i32,
    pub score_per_level: i32,
    pub shutout_bonus_levels: u8,
    pub bottom_card_count: usize,
    pub trump_suit: Option<Suit>,
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
    pub round_index: i32,
    pub dealt_count: usize,
    pub total_deal_count: usize,
    pub declaration: Option<WsUpgradeTrumpDeclaration>,
    pub current_trick: Vec<share_type_public::WsUpgradePlayedCards>,
    pub trick_index: i32,
    pub player_scores: HashMap<usize, i32>,
    pub collected_scores: HashMap<usize, i32>,
    pub buried: bool,
}

pub type UpgradeStateHandle = Arc<Mutex<UpgradeGameState>>;

pub fn build_upgrade_deck(deck_count: UpgradeDeckCount) -> Vec<i32> {
    (0..usize::from(deck_count.get()))
        .flat_map(|deck| {
            let offset = (deck as i32) * 100;
            (1..=54).map(move |identity| offset + identity)
        })
        .collect()
}

fn rank_to_protocol(rank: Rank) -> UpgradeRank {
    match rank {
        Rank::Two => UpgradeRank::TWO,
        Rank::Three => UpgradeRank::THREE,
        Rank::Four => UpgradeRank::FOUR,
        Rank::Five => UpgradeRank::FIVE,
        Rank::Six => UpgradeRank::SIX,
        Rank::Seven => UpgradeRank::SEVEN,
        Rank::Eight => UpgradeRank::EIGHT,
        Rank::Nine => UpgradeRank::NINE,
        Rank::Ten => UpgradeRank::TEN,
        Rank::Jack => UpgradeRank::J,
        Rank::Queen => UpgradeRank::Q,
        Rank::King => UpgradeRank::K,
        Rank::Ace => UpgradeRank::A,
        Rank::SmallJoker | Rank::BigJoker => UpgradeRank::TWO,
    }
}

fn suit_to_protocol(suit: Suit) -> UpgradeSuit {
    match suit {
        Suit::Spade => UpgradeSuit::SPADE,
        Suit::Heart => UpgradeSuit::HEART,
        Suit::Club => UpgradeSuit::CLUB,
        Suit::Diamond => UpgradeSuit::DIAMOND,
    }
}

fn card_from_id(card: i32) -> Option<Card> {
    Card::try_from(card).ok()
}

fn remove_cards(hand: &mut Vec<i32>, cards: &[i32]) -> Result<(), &'static str> {
    let mut indexes = Vec::with_capacity(cards.len());
    for card in cards {
        let Some(index) = hand.iter().enumerate().find_map(|(index, current)| {
            (!indexes.contains(&index) && current == card).then_some(index)
        }) else {
            return Err("card is not in hand");
        };
        indexes.push(index);
    }
    indexes.sort_unstable_by(|left, right| right.cmp(left));
    for index in indexes {
        hand.remove(index);
    }
    Ok(())
}

fn default_bottom_count(total_cards: usize) -> usize {
    // 54*N must leave a multiple of four cards. Keep a standard visible
    // bottom size while adapting odd deck counts to the four-player table.
    if total_cards.is_multiple_of(4) { 8 } else { 10 }
}

impl UpgradeGameState {
    pub fn from_common(common: Arc<Mutex<CommonGameState>>) -> Self {
        Self {
            base: common,
            phase: UpgradePhase::Start,
            rules: UpgradeRules {
                deck_count: UpgradeDeckCount::new(3).expect("valid default deck count"),
                target_rank: Rank::Three,
                final_target_rank: Rank::Ace,
                attacking_win_score: 80,
                score_per_level: 40,
                shutout_bonus_levels: 1,
                bottom_card_count: 10,
                trump_suit: None,
            },
            hands: HashMap::new(),
            bottom_cards: Vec::new(),
            dealer_position: 0,
            current_position: 0,
            round_index: 0,
            dealt_count: 0,
            total_deal_count: 0,
            declaration: None,
            current_trick: Vec::new(),
            trick_index: 0,
            player_scores: HashMap::new(),
            collected_scores: HashMap::new(),
            buried: false,
        }
    }

    pub fn deal_new_round(&mut self, rules: UpgradeRules) -> Result<(), &'static str> {
        let mut deck = build_upgrade_deck(rules.deck_count);
        let total_cards = deck.len();
        let bottom_count = rules
            .bottom_card_count
            .max(default_bottom_count(total_cards));
        if total_cards <= bottom_count || !(total_cards - bottom_count).is_multiple_of(PLAYER_COUNT)
        {
            return Err("deck cannot be dealt evenly");
        }
        deck.shuffle(&mut rand::rng());
        self.rules = UpgradeRules {
            bottom_card_count: bottom_count,
            ..rules
        };
        self.phase = UpgradePhase::Bury;
        self.hands.clear();
        self.bottom_cards = deck.split_off(total_cards - bottom_count);
        let hand_count = deck.len() / PLAYER_COUNT;
        for position in 0..PLAYER_COUNT {
            let start = position * hand_count;
            let end = start + hand_count;
            let mut hand = deck[start..end].to_vec();
            hand.sort_unstable_by_key(|card| {
                card_from_id(*card)
                    .map(|card| (card.suit().is_none(), card.rank(), card.encoded()))
                    .unwrap_or((true, Rank::Two, *card))
            });
            self.hands.insert(position, hand);
        }
        // The dealer sees the bottom and may choose cards from the combined hand.
        self.hands
            .entry(self.dealer_position)
            .or_default()
            .extend(self.bottom_cards.iter().copied());
        self.hands
            .entry(self.dealer_position)
            .and_modify(|hand| hand.sort_unstable());
        self.dealt_count = total_cards;
        self.total_deal_count = total_cards;
        self.declaration = None;
        self.current_trick.clear();
        self.trick_index = 0;
        self.collected_scores.clear();
        self.buried = false;
        self.current_position = self.dealer_position;
        Ok(())
    }

    pub fn hand_count(&self) -> usize {
        let total = self.total_deal_count;
        total.saturating_sub(self.rules.bottom_card_count) / PLAYER_COUNT
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn bury_bottom(&mut self, position: usize, cards: Vec<i32>) -> Result<(), &'static str> {
        if self.phase != UpgradePhase::Bury
            || self.buried
            || position != self.dealer_position
            || cards.len() != self.rules.bottom_card_count
        {
            return Err("not allowed to bury bottom");
        }
        let hand = self.hands.get_mut(&position).ok_or("dealer hand missing")?;
        remove_cards(hand, &cards)?;
        self.bottom_cards = cards;
        self.buried = true;
        Ok(())
    }

    pub fn select_trump(&mut self, position: usize, suit: UpgradeSuit) -> Result<(), &'static str> {
        if self.phase != UpgradePhase::Bury || !self.buried || position != self.dealer_position {
            return Err("not allowed to select trump");
        }
        let suit = match suit {
            UpgradeSuit::SPADE => Suit::Spade,
            UpgradeSuit::HEART => Suit::Heart,
            UpgradeSuit::CLUB => Suit::Club,
            UpgradeSuit::DIAMOND => Suit::Diamond,
        };
        self.rules.trump_suit = Some(suit);
        self.phase = UpgradePhase::Play;
        self.current_position = self.dealer_position;
        Ok(())
    }

    pub fn snapshot(&self) -> WsUpgradeTableSnapshotEvent {
        WsUpgradeTableSnapshotEvent {
            phase: self.phase,
            deck_count: i32::from(self.rules.deck_count.get()),
            target_rank: rank_to_protocol(self.rules.target_rank),
            final_target_rank: rank_to_protocol(self.rules.final_target_rank),
            removed_rank_count: 0,
            round_index: self.round_index,
            attacking_win_score: self.rules.attacking_win_score,
            score_per_level: self.rules.score_per_level,
            shutout_bonus_levels: i32::from(self.rules.shutout_bonus_levels),
            bottom_card_count: self.rules.bottom_card_count as i32,
            hand_count: self.hand_count() as i32,
            dealer_position: self.dealer_position as i32,
            trump_suit: self.rules.trump_suit.map(suit_to_protocol),
            declaration: self.declaration.clone(),
            dealt_count: self.dealt_count as i32,
            total_deal_count: self.total_deal_count as i32,
            player_hand_counts: (0..PLAYER_COUNT)
                .map(|position| WsUpgradePlayerHandCount {
                    position: position as i32,
                    hand_count: self.hands.get(&position).map_or(0, Vec::len) as i32,
                })
                .collect(),
            current_position: self.current_position as i32,
            trick_index: self.trick_index,
            current_trick: self.current_trick.clone(),
            turn_countdown: self.base.lock().unwrap().turn_countdown as i32,
            player_scores: self
                .player_scores
                .iter()
                .map(|(position, score)| (*position as i32, *score))
                .collect(),
            failed_throws: Vec::new(),
        }
    }

    pub fn exposed_bottom(&self) -> Vec<i32> {
        self.bottom_cards.clone()
    }

    pub fn private_hand(&self, position: usize) -> Vec<i32> {
        self.hands.get(&position).cloned().unwrap_or_default()
    }
}

impl GameState for UpgradeGameState {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.base)
    }

    fn can_accept_players(&self) -> bool {
        self.phase == UpgradePhase::Start
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
