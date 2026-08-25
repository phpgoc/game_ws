//! 升级牌堆、底牌、主牌声明和四人队伍的运行态。

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use rand::{Rng, seq::SliceRandom};
use share_type_public::{
    UpgradePhase, UpgradeRank, UpgradeSuit, WsUpgradeFailedThrowEvent, WsUpgradePlayedCards,
    WsUpgradePlayerHandCount, WsUpgradeSettlementEvent, WsUpgradeTableSnapshotEvent,
    WsUpgradeTrumpDeclaration,
};
use upgrade_common::{
    Card, Rank, ScoreOutcome, ScoreProgression, ScoreSide, Suit, level_rank_path,
    next_four_player_dealer, next_level_rank,
};
use ws_common::{CommonGameState, GameState};

use crate::{
    UpgradeDeckCount,
    combo::{self, ComboKind, UpgradeComboRules},
};

pub const PLAYER_COUNT: usize = 4;

/// 按设置数字依次移除 3、4、6、7、8、9；其余牌面始终保留。
pub const REMOVABLE_RANKS: [Rank; 6] = [
    Rank::Three,
    Rank::Four,
    Rank::Six,
    Rank::Seven,
    Rank::Eight,
    Rank::Nine,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeRules {
    pub deck_count: UpgradeDeckCount,
    pub target_rank: Rank,
    pub final_target_rank: Rank,
    pub removed_rank_count: usize,
    pub attacking_win_score: i32,
    pub score_per_level: i32,
    pub shutout_bonus_levels: u8,
    pub bottom_card_count: usize,
    pub trump_suit: Option<Suit>,
}

#[derive(Debug)]
pub struct UpgradeGameState {
    /// 牌局循环独占的可变事实，外层 handle 只提供线程安全共享。
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: UpgradePhase,
    pub rules: UpgradeRules,
    pub team_target_ranks: [Rank; 2],
    pub hands: HashMap<usize, Vec<i32>>,
    pub bottom_cards: Vec<i32>,
    pub deal_queue: VecDeque<(usize, i32)>,
    pub dealer_position: usize,
    pub current_position: usize,
    pub round_index: i32,
    pub dealt_count: usize,
    pub total_deal_count: usize,
    pub declaration: Option<WsUpgradeTrumpDeclaration>,
    pub current_trick: Vec<WsUpgradePlayedCards>,
    pub play_history: Vec<WsUpgradePlayedCards>,
    pub trick_index: i32,
    pub player_scores: HashMap<usize, i32>,
    pub collected_scores: HashMap<usize, i32>,
    pub buried: bool,
    pub last_trick_winner: Option<usize>,
    pub bottom_multiplier: usize,
    pub failed_throws: Vec<FailedThrow>,
}

pub type UpgradeStateHandle = Arc<Mutex<UpgradeGameState>>;

#[derive(Debug, Clone)]
pub struct FailedThrow {
    /// 甩牌失败时保留尝试牌和实际拆出的组件，用于事件展示和重试。
    pub position: usize,
    pub attempted_cards: Vec<i32>,
    pub played_cards: Vec<i32>,
    pub play_sequence: usize,
}

#[derive(Debug, Clone)]
pub struct PlayResolution {
    pub attempted_cards: Vec<i32>,
    pub played_cards: Vec<i32>,
    pub failed_throw: Option<WsUpgradeFailedThrowEvent>,
    pub winner: Option<usize>,
    pub finished: bool,
}

pub fn build_upgrade_deck(deck_count: UpgradeDeckCount) -> Vec<i32> {
    // 牌堆生成必须和协议卡牌 ID 一致；副数和移除牌面由同一个函数处理。
    build_upgrade_deck_with_removed_ranks(deck_count, 0)
}

pub fn removed_upgrade_ranks(removed_rank_count: usize) -> &'static [Rank] {
    &REMOVABLE_RANKS[..removed_rank_count.min(REMOVABLE_RANKS.len())]
}

pub fn first_upgrade_rank(removed_rank_count: usize, final_rank: Rank) -> Rank {
    level_rank_path(final_rank, removed_upgrade_ranks(removed_rank_count))
        .first()
        .copied()
        .unwrap_or(final_rank)
}

pub fn build_upgrade_deck_with_removed_ranks(
    deck_count: UpgradeDeckCount,
    removed_rank_count: usize,
) -> Vec<i32> {
    let removed = removed_upgrade_ranks(removed_rank_count);
    (0..usize::from(deck_count.get()))
        .flat_map(|deck| {
            let offset = (deck as i32) * 100;
            (1..=54).filter_map(move |identity| {
                let encoded = offset + identity;
                let rank = Card::try_from(encoded).ok()?.rank();
                (!removed.contains(&rank)).then_some(encoded)
            })
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
                removed_rank_count: 0,
                attacking_win_score: 80,
                score_per_level: 40,
                shutout_bonus_levels: 1,
                bottom_card_count: 10,
                trump_suit: None,
            },
            team_target_ranks: [Rank::Three; 2],
            hands: HashMap::new(),
            bottom_cards: Vec::new(),
            deal_queue: VecDeque::new(),
            dealer_position: 0,
            current_position: 0,
            round_index: 0,
            dealt_count: 0,
            total_deal_count: 0,
            declaration: None,
            current_trick: Vec::new(),
            play_history: Vec::new(),
            trick_index: 0,
            player_scores: HashMap::new(),
            collected_scores: HashMap::new(),
            buried: false,
            last_trick_winner: None,
            bottom_multiplier: 1,
            failed_throws: Vec::new(),
        }
    }

    pub fn deal_new_round(&mut self, rules: UpgradeRules) -> Result<(), &'static str> {
        self.deal_new_round_with_rng(rules, &mut rand::rng())
    }

    pub fn deal_new_round_with_rng<R: Rng + ?Sized>(
        &mut self,
        mut rules: UpgradeRules,
        rng: &mut R,
    ) -> Result<(), &'static str> {
        rules.removed_rank_count = rules.removed_rank_count.min(REMOVABLE_RANKS.len());
        if self.round_index == 0 {
            rules.target_rank =
                first_upgrade_rank(rules.removed_rank_count, rules.final_target_rank);
        }
        let mut deck =
            build_upgrade_deck_with_removed_ranks(rules.deck_count, rules.removed_rank_count);
        let total_cards = deck.len();
        let bottom_count = rules
            .bottom_card_count
            .max(default_bottom_count(total_cards));
        if total_cards <= bottom_count || !(total_cards - bottom_count).is_multiple_of(PLAYER_COUNT)
        {
            return Err("deck cannot be dealt evenly");
        }
        deck.shuffle(rng);
        if self.round_index == 0 {
            self.team_target_ranks = [rules.target_rank; 2];
        }
        self.rules = UpgradeRules {
            bottom_card_count: bottom_count,
            trump_suit: None,
            ..rules
        };
        self.phase = UpgradePhase::Deal;
        self.hands.clear();
        self.bottom_cards = deck.split_off(total_cards - bottom_count);
        for position in 0..PLAYER_COUNT {
            self.hands.insert(position, Vec::new());
        }
        self.deal_queue = deck
            .into_iter()
            .enumerate()
            .map(|(index, card)| (index % PLAYER_COUNT, card))
            .collect();
        self.dealt_count = 0;
        self.total_deal_count = self.deal_queue.len();
        self.declaration = None;
        self.current_trick.clear();
        self.play_history.clear();
        self.trick_index = 0;
        self.collected_scores.clear();
        self.last_trick_winner = None;
        self.bottom_multiplier = 1;
        self.failed_throws.clear();
        self.buried = false;
        self.current_position = self.dealer_position;
        Ok(())
    }

    pub fn hand_count(&self) -> usize {
        self.total_deal_count / PLAYER_COUNT
    }

    /// Deals one private card. The first round accepts declarations while the
    /// queue is moving; when nobody declares, the first available level card
    /// is revealed at the end so the opening round never enters a second
    /// post-bottom suit-selection phase.
    pub fn deal_next_card(
        &mut self,
    ) -> Option<(usize, i32, bool, Option<WsUpgradeTrumpDeclaration>)> {
        if self.phase != UpgradePhase::Deal {
            return None;
        }
        let (position, card) = self.deal_queue.pop_front()?;
        let hand = self.hands.entry(position).or_default();
        hand.push(card);
        hand.sort_unstable_by_key(|card| {
            card_from_id(*card)
                .map(|card| (card.suit().is_none(), card.rank(), card.encoded()))
                .unwrap_or((true, Rank::Two, *card))
        });
        self.dealt_count += 1;
        let finished = self.deal_queue.is_empty();
        let controlled = {
            let base = self.base.lock().unwrap();
            base.is_ai_position(position) || base.is_ai_takeover_position(position)
        };
        let current_strength = self
            .declaration
            .as_ref()
            .map_or(0, |declaration| declaration.strength.max(0) as usize);
        let mut automatic_declaration = (self.round_index == 0 && controlled)
            .then(|| crate::ai::declaration_cards(self, position, current_strength))
            .flatten()
            .and_then(|cards| self.declare_trump(position, cards).ok());
        if finished {
            if self.round_index == 0 && self.declaration.is_none() {
                automatic_declaration = (0..PLAYER_COUNT).find_map(|candidate_position| {
                    let card =
                        self.hands
                            .get(&candidate_position)?
                            .iter()
                            .copied()
                            .find(|card| {
                                Card::try_from(*card).ok().is_some_and(|card| {
                                    card.rank() == self.rules.target_rank && card.suit().is_some()
                                })
                            })?;
                    self.declare_trump(candidate_position, vec![card]).ok()
                });
            }
            if self.round_index == 0 && self.declaration.is_none() {
                automatic_declaration = self.declare_bottom_level_fallback();
            }
            if let Some(declaration) = &self.declaration {
                self.dealer_position = declaration.position as usize;
            }
            self.current_position = self.dealer_position;
            self.hands
                .entry(self.dealer_position)
                .or_default()
                .extend(self.bottom_cards.iter().copied());
            self.hands
                .entry(self.dealer_position)
                .and_modify(|hand| hand.sort_unstable());
            self.phase = UpgradePhase::Bury;
            self.base.lock().unwrap().action_received = false;
        }
        Some((position, card, finished, automatic_declaration))
    }

    fn declare_bottom_level_fallback(&mut self) -> Option<WsUpgradeTrumpDeclaration> {
        let position = self.dealer_position;
        let card = self.bottom_cards.iter().copied().find(|card| {
            Card::try_from(*card)
                .ok()
                .is_some_and(|card| card.rank() == self.rules.target_rank && card.suit().is_some())
        })?;
        let first = Card::try_from(card).ok()?;
        let suit = first.suit()?;
        let declaration = WsUpgradeTrumpDeclaration {
            position: position as i32,
            name: self.player_name(position),
            cards: vec![card],
            trump_suit: suit_to_protocol(suit),
            strength: 1,
            target_rank: rank_to_protocol(self.rules.target_rank),
        };
        self.rules.trump_suit = Some(suit);
        self.current_position = position;
        self.declaration = Some(declaration.clone());
        Some(declaration)
    }

    pub fn declare_trump(
        &mut self,
        position: usize,
        cards: Vec<i32>,
    ) -> Result<WsUpgradeTrumpDeclaration, &'static str> {
        if self.round_index != 0 || self.phase != UpgradePhase::Deal || cards.is_empty() {
            return Err("not in first-round deal");
        }
        let hand = self.hands.get(&position).cloned().unwrap_or_default();
        if !contains_cards(&hand, &cards) {
            return Err("declaration card not dealt");
        }
        let first = Card::try_from(cards[0]).map_err(|_| "invalid declaration card")?;
        let suit = first.suit().ok_or("joker cannot declare trump")?;
        if first.rank() != self.rules.target_rank
            || cards.iter().any(|card| {
                Card::try_from(*card).ok().is_none_or(|card| {
                    card.identity() != first.identity() || card.rank() != self.rules.target_rank
                })
            })
        {
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
        let declaration = WsUpgradeTrumpDeclaration {
            position: position as i32,
            name: self.player_name(position),
            cards,
            trump_suit: suit_to_protocol(suit),
            strength,
            target_rank: rank_to_protocol(self.rules.target_rank),
        };
        self.rules.trump_suit = Some(suit);
        self.dealer_position = position;
        self.current_position = position;
        self.declaration = Some(declaration.clone());
        Ok(declaration)
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn bury_bottom(&mut self, position: usize, cards: Vec<i32>) -> Result<(), &'static str> {
        if self.phase != UpgradePhase::Bury
            || self.buried
            || position != self.dealer_position
            || self.rules.trump_suit.is_none()
            || cards.len() != self.rules.bottom_card_count
        {
            return Err("not allowed to bury bottom");
        }
        let hand = self.hands.get_mut(&position).ok_or("dealer hand missing")?;
        remove_cards(hand, &cards)?;
        self.bottom_cards = cards;
        self.buried = true;
        if self.rules.trump_suit.is_some() {
            self.phase = UpgradePhase::Play;
        }
        Ok(())
    }

    pub fn select_trump(&mut self, position: usize, suit: UpgradeSuit) -> Result<(), &'static str> {
        if self.phase != UpgradePhase::Bury
            || self.round_index == 0
            || self.buried
            || position != self.dealer_position
            || self.rules.trump_suit.is_some()
        {
            return Err("not allowed to select trump");
        }
        let suit = match suit {
            UpgradeSuit::SPADE => Suit::Spade,
            UpgradeSuit::HEART => Suit::Heart,
            UpgradeSuit::CLUB => Suit::Club,
            UpgradeSuit::DIAMOND => Suit::Diamond,
        };
        self.rules.trump_suit = Some(suit);
        Ok(())
    }

    pub fn snapshot(&self) -> WsUpgradeTableSnapshotEvent {
        WsUpgradeTableSnapshotEvent {
            phase: self.phase,
            deck_count: i32::from(self.rules.deck_count.get()),
            target_rank: rank_to_protocol(self.rules.target_rank),
            team_target_ranks: self
                .team_target_ranks
                .into_iter()
                .map(rank_to_protocol)
                .collect(),
            final_target_rank: rank_to_protocol(self.rules.final_target_rank),
            removed_rank_count: self.rules.removed_rank_count as i32,
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
            failed_throws: self
                .failed_throws
                .iter()
                .map(|failed| WsUpgradeFailedThrowEvent {
                    position: failed.position as i32,
                    attempted_cards: failed.attempted_cards.clone(),
                    played_cards: failed.played_cards.clone(),
                })
                .collect(),
        }
    }

    pub fn exposed_bottom(&self) -> Vec<i32> {
        self.bottom_cards.clone()
    }

    pub fn target_rank_protocol(&self) -> UpgradeRank {
        rank_to_protocol(self.rules.target_rank)
    }

    pub fn private_hand(&self, position: usize) -> Vec<i32> {
        self.hands.get(&position).cloned().unwrap_or_default()
    }

    fn combo_rules(&self) -> UpgradeComboRules {
        UpgradeComboRules {
            target_rank: self.rules.target_rank,
            trump_suit: self.rules.trump_suit,
        }
    }

    fn cards_from_ids(cards: &[i32]) -> Result<Vec<Card>, &'static str> {
        cards
            .iter()
            .copied()
            .map(|card| Card::try_from(card).map_err(|_| "invalid card"))
            .collect()
    }

    fn opponent_hands(&self, position: usize) -> impl Iterator<Item = &[i32]> {
        self.hands
            .iter()
            .filter_map(move |(other, hand)| (*other != position).then_some(hand.as_slice()))
    }

    fn winner_for_trick(&self) -> Option<usize> {
        let lead = self.current_trick.first()?;
        let rules = self.combo_rules();
        let lead_cards = Self::cards_from_ids(&lead.cards).ok()?;
        let lead_combo = combo::classify(&lead_cards, rules)?;
        let mut winner = usize::try_from(lead.position).ok()?;
        let mut best_priority = if lead_combo.group.is_none() { 2 } else { 1 };
        let mut best = combo::combo_win_value(&lead_cards, &lead_combo, rules)?;
        for played in self.current_trick.iter().skip(1) {
            let cards = Self::cards_from_ids(&played.cards).ok()?;
            let Some(candidate) = combo::classify(&cards, rules) else {
                continue;
            };
            let competes = match (lead_combo.group, candidate.group) {
                (Some(lead_group), Some(candidate_group)) => lead_group == candidate_group,
                (None, None) | (Some(_), None) => true,
                (None, Some(_)) => false,
            };
            if !competes {
                continue;
            }
            let priority = if candidate.group.is_none() { 2 } else { 1 };
            let Some(value) = combo::combo_win_value(&cards, &lead_combo, rules) else {
                continue;
            };
            if priority > best_priority || (priority == best_priority && value > best) {
                best_priority = priority;
                best = value;
                winner = usize::try_from(played.position).ok()?;
            }
        }
        Some(winner)
    }

    fn trick_points(trick: &[WsUpgradePlayedCards]) -> i32 {
        trick
            .iter()
            .flat_map(|played| played.cards.iter())
            .filter_map(|card| Card::try_from(*card).ok())
            .map(|card| i32::from(card.points()))
            .sum()
    }

    pub fn play_cards(
        &mut self,
        position: usize,
        cards: Vec<i32>,
    ) -> Result<PlayResolution, &'static str> {
        if self.phase != UpgradePhase::Play || self.current_position != position || cards.is_empty()
        {
            return Err("not current turn");
        }
        let hand = self.private_hand(position);
        let attempted_cards = cards.clone();
        let rules = self.combo_rules();
        let mut played_cards = cards;
        let mut failed_throw = None;
        let decoded = Self::cards_from_ids(&played_cards)?;
        if self.current_trick.is_empty() {
            let Some(lead_combo) = combo::classify(&decoded, rules) else {
                return Err("invalid play shape");
            };
            if !contains_cards(&hand, &played_cards) {
                return Err("card is not in hand");
            }
            if matches!(lead_combo.kind, ComboKind::Throw { .. })
                && let Some(fallback) = self
                    .opponent_hands(position)
                    .filter_map(|opponent| {
                        let opponent_cards = Self::cards_from_ids(opponent).ok()?;
                        combo::failed_throw_component(&decoded, &opponent_cards, rules)
                    })
                    .min_by_key(|component| {
                        (
                            component
                                .first()
                                .map(|card| combo::card_strength(*card, rules))
                                .unwrap_or_default(),
                            component.len(),
                            component
                                .first()
                                .map(|card| card.encoded())
                                .unwrap_or_default(),
                        )
                    })
            {
                let fallback_ids: Vec<i32> = fallback.iter().map(|card| card.encoded()).collect();
                failed_throw = Some(WsUpgradeFailedThrowEvent {
                    position: position as i32,
                    attempted_cards: attempted_cards.clone(),
                    played_cards: fallback_ids.clone(),
                });
                played_cards = fallback_ids;
            }
        } else {
            let lead_cards = Self::cards_from_ids(&self.current_trick[0].cards)?;
            let Some(lead_combo) = combo::classify(&lead_cards, rules) else {
                return Err("invalid lead");
            };
            if !combo::follow_is_legal(
                &Self::cards_from_ids(&hand)?,
                &Self::cards_from_ids(&played_cards)?,
                &lead_combo,
                rules,
            ) {
                return Err("illegal follow");
            }
        }
        remove_cards(
            self.hands.get_mut(&position).ok_or("hand missing")?,
            &played_cards,
        )?;
        let played = WsUpgradePlayedCards {
            position: position as i32,
            name: self.player_name(position),
            cards: played_cards.clone(),
        };
        if let Some(failed) = &failed_throw {
            self.failed_throws.push(FailedThrow {
                position,
                attempted_cards: attempted_cards.clone(),
                played_cards: played_cards.clone(),
                play_sequence: self.trick_index as usize,
            });
            let _ = failed;
        }
        self.current_trick.push(played.clone());
        self.play_history.push(played);
        let mut winner = None;
        if self.current_trick.len() == PLAYER_COUNT {
            winner = self.winner_for_trick();
            let winning_cards = winner.and_then(|winner| {
                self.current_trick
                    .iter()
                    .find(|played| played.position == winner as i32)
                    .map(|played| played.cards.clone())
            });
            if let Some(winner) = winner {
                *self.collected_scores.entry(winner).or_default() +=
                    Self::trick_points(&self.current_trick);
            }
            self.bottom_multiplier = winning_cards
                .as_ref()
                .and_then(|cards| Self::cards_from_ids(cards).ok())
                .map(|cards| combo::bottom_multiplier(&cards))
                .unwrap_or(1);
            self.last_trick_winner = winner;
            self.current_trick.clear();
            self.trick_index += 1;
            if let Some(winner) = winner {
                self.current_position = winner;
            }
            if self.hands.values().all(Vec::is_empty) {
                if let Some(winner) = self.last_trick_winner {
                    let bottom_score = Self::trick_points(&[WsUpgradePlayedCards {
                        position: winner as i32,
                        name: String::new(),
                        cards: self.bottom_cards.clone(),
                    }]) * self.bottom_multiplier as i32;
                    *self.collected_scores.entry(winner).or_default() += bottom_score;
                }
                self.phase = UpgradePhase::Settlement;
                self.record_settlement_scores();
            }
        } else {
            self.current_position = (position + 1) % PLAYER_COUNT;
        }
        self.base.lock().unwrap().action_received = true;
        Ok(PlayResolution {
            attempted_cards,
            played_cards,
            failed_throw,
            winner,
            finished: self.phase == UpgradePhase::Settlement,
        })
    }

    fn attacking_score(&self) -> i32 {
        [
            (self.dealer_position + 1) % PLAYER_COUNT,
            (self.dealer_position + 3) % PLAYER_COUNT,
        ]
        .iter()
        .map(|position| {
            self.collected_scores
                .get(position)
                .copied()
                .unwrap_or_default()
        })
        .sum()
    }

    fn score_outcome(&self) -> ScoreOutcome {
        ScoreProgression::new(
            self.rules.attacking_win_score.max(1) as u32,
            self.rules.score_per_level.max(1) as u32,
            self.rules.shutout_bonus_levels,
        )
        .expect("normalized score progression")
        .outcome(self.attacking_score())
    }

    fn winner_positions_usize(&self) -> Vec<usize> {
        match self.score_outcome().side {
            ScoreSide::Attacking => vec![
                (self.dealer_position + 1) % PLAYER_COUNT,
                (self.dealer_position + 3) % PLAYER_COUNT,
            ],
            ScoreSide::Defending => vec![
                self.dealer_position,
                (self.dealer_position + 2) % PLAYER_COUNT,
            ],
        }
    }

    fn record_settlement_scores(&mut self) {
        let score = self.attacking_score();
        let winners = self.winner_positions_usize();
        for position in 0..PLAYER_COUNT {
            let delta = if winners.contains(&position) {
                score
            } else {
                -score
            };
            *self.player_scores.entry(position).or_default() += delta;
        }
    }

    pub fn settlement_event(&self) -> WsUpgradeSettlementEvent {
        let outcome = self.score_outcome();
        let next_target_rank = self.next_target_rank();
        let winning_team = self.winner_positions_usize()[0] % 2;
        let mut team_target_ranks = self.team_target_ranks;
        if let Some(next_target_rank) = next_target_rank {
            team_target_ranks[winning_team] = next_target_rank;
        }
        WsUpgradeSettlementEvent {
            winner_positions: self
                .winner_positions_usize()
                .into_iter()
                .map(|position| position as i32)
                .collect(),
            score: self.attacking_score(),
            level_change: i32::from(outcome.levels),
            target_rank: rank_to_protocol(self.rules.target_rank),
            match_finished: next_target_rank.is_none(),
            next_target_rank: next_target_rank.map(rank_to_protocol),
            team_target_ranks: team_target_ranks
                .into_iter()
                .map(rank_to_protocol)
                .collect(),
            player_scores: self
                .player_scores
                .iter()
                .map(|(position, score)| (*position as i32, *score))
                .collect(),
        }
    }

    pub fn advance_after_settlement(&mut self) -> Result<bool, &'static str> {
        self.advance_after_settlement_with_rng(&mut rand::rng())
    }

    pub fn advance_after_settlement_with_rng<R: Rng + ?Sized>(
        &mut self,
        rng: &mut R,
    ) -> Result<bool, &'static str> {
        if self.phase != UpgradePhase::Settlement {
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
        self.round_index += 1;
        self.rules.target_rank = next_rank;
        self.rules.trump_suit = None;
        self.deal_new_round_with_rng(self.rules, rng)?;
        Ok(true)
    }

    /// Return a finished match to the room lobby without replacing the
    /// shared room membership. The next START request creates a fresh game
    /// state while the current clients receive an authoritative empty table.
    pub fn reset_to_lobby(&mut self) {
        self.phase = UpgradePhase::Start;
        self.rules.trump_suit = None;
        self.rules.target_rank =
            first_upgrade_rank(self.rules.removed_rank_count, self.rules.final_target_rank);
        self.team_target_ranks = [self.rules.target_rank; 2];
        self.hands.clear();
        self.bottom_cards.clear();
        self.deal_queue.clear();
        self.dealer_position = 0;
        self.current_position = 0;
        self.round_index = 0;
        self.dealt_count = 0;
        self.total_deal_count = 0;
        self.declaration = None;
        self.current_trick.clear();
        self.play_history.clear();
        self.trick_index = 0;
        self.player_scores.clear();
        self.collected_scores.clear();
        self.buried = false;
        self.last_trick_winner = None;
        self.bottom_multiplier = 1;
        self.failed_throws.clear();
        self.set_turn_countdown(0);
        self.base.lock().unwrap().action_received = false;
    }

    fn next_target_rank(&self) -> Option<Rank> {
        let winning_team = self.winner_positions_usize()[0] % 2;
        next_level_rank(
            self.team_target_ranks[winning_team],
            self.rules.final_target_rank,
            removed_upgrade_ranks(self.rules.removed_rank_count),
            usize::from(self.score_outcome().levels),
        )
    }

    pub fn timeout_bury(&mut self) -> Result<bool, &'static str> {
        if self.phase != UpgradePhase::Bury || self.current_position != self.dealer_position {
            return Ok(false);
        }
        if self.round_index > 0 && self.rules.trump_suit.is_none() {
            let suit = crate::ai::best_trump_suit(self, self.dealer_position);
            self.select_trump(self.dealer_position, suit)?;
        }
        let cards = crate::ai::choose_bury(self).or_else(|| self.choose_fallback_bury());
        let Some(cards) = cards else {
            return Ok(false);
        };
        if cards.len() != self.rules.bottom_card_count {
            return Ok(false);
        }
        self.bury_bottom(self.dealer_position, cards)?;
        Ok(true)
    }

    pub fn timeout_play(&mut self) -> Result<Option<PlayResolution>, &'static str> {
        if self.phase != UpgradePhase::Play {
            return Ok(None);
        }
        let position = self.current_position;
        let Some(cards) =
            crate::ai::decide(self, position).or_else(|| self.choose_fallback_play(position))
        else {
            return Ok(None);
        };
        self.play_cards(position, cards).map(Some)
    }

    pub(crate) fn choose_fallback_bury(&self) -> Option<Vec<i32>> {
        let cards = self
            .private_hand(self.dealer_position)
            .into_iter()
            .take(self.rules.bottom_card_count)
            .collect::<Vec<_>>();
        (cards.len() == self.rules.bottom_card_count).then_some(cards)
    }

    pub(crate) fn choose_fallback_play(&self, position: usize) -> Option<Vec<i32>> {
        let hand = self.private_hand(position);
        let first = hand.first().copied()?;
        let cards = if self.current_trick.is_empty() {
            vec![first]
        } else {
            let lead = Self::cards_from_ids(&self.current_trick[0].cards).ok()?;
            let rules = self.combo_rules();
            let lead_combo = combo::classify(&lead, rules)?;
            combo::forced_follow(&Self::cards_from_ids(&hand).ok()?, &lead_combo, rules)?
                .into_iter()
                .map(Card::encoded)
                .collect()
        };
        Some(cards)
    }
}

fn contains_cards(hand: &[i32], cards: &[i32]) -> bool {
    let mut available = hand.to_vec();
    for card in cards {
        let Some(index) = available.iter().position(|candidate| candidate == card) else {
            return false;
        };
        available.remove(index);
    }
    true
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
