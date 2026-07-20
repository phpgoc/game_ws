mod belief;
mod bidding;
mod candidates;
mod playing;
mod search;
#[cfg(test)]
mod simulation;

use std::collections::BTreeMap;

use share_type_public::LandlordPhase;

pub use belief::{CardBelief, FarmerRunnerEstimate, OpponentEstimate};

use crate::core::play::ComboKind;
use crate::game_state::{LandlordLoopState, LandlordPlayRecord};

use self::candidates::all_candidates;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relationship {
    SelfPlayer,
    Ally,
    Enemy,
}

/// AI 的完整可观察信息。这里故意不保存其他玩家的手牌内容；
/// `hand_sizes` 只保留公开的剩余张数。
#[derive(Clone, Debug, PartialEq)]
pub struct AiObservation {
    pub position: usize,
    pub phase: LandlordPhase,
    pub hand: Vec<i32>,
    pub positions: Vec<usize>,
    pub hand_sizes: BTreeMap<usize, usize>,
    pub landlord_position: Option<usize>,
    pub current_position: usize,
    pub current_score: u8,
    pub call_history: Vec<(usize, u8)>,
    pub hidden_cards: Vec<i32>,
    pub last_play_position: usize,
    pub last_play: Vec<i32>,
    pub play_history: Vec<LandlordPlayRecord>,
    /// 只供两名 AI 农民协调角色的队友炸弹延迟信号。
    pub ai_bomb_signal_position: Option<usize>,
}

impl AiObservation {
    pub fn from_state(state: &LandlordLoopState, position: usize) -> Option<Self> {
        let hand = state.hands.get(&position)?.clone();
        let mut positions = state.players_snapshot().keys().copied().collect::<Vec<_>>();
        positions.sort_unstable();
        let hand_sizes = state
            .hands
            .iter()
            .map(|(&position, cards)| (position, cards.len()))
            .collect();
        let hidden_cards = matches!(state.phase, LandlordPhase::Play | LandlordPhase::Settlement)
            .then(|| state.hidden_cards.clone())
            .unwrap_or_default();
        let ai_bomb_signal_position = state.ai_bomb_signal_position.filter(|signal_position| {
            state.is_ai_controlled_position(position)
                && state
                    .landlord_position
                    .is_some_and(|landlord| landlord != position && landlord != *signal_position)
        });
        Some(Self {
            position,
            phase: state.phase,
            hand,
            positions,
            hand_sizes,
            landlord_position: state.landlord_position,
            current_position: state.current_position,
            current_score: state.score as u8,
            call_history: state.call_history.clone(),
            hidden_cards,
            last_play_position: state.last_play_position,
            last_play: state.last_play.clone(),
            play_history: state.play_history.clone(),
            ai_bomb_signal_position,
        })
    }

    pub fn relationship_to(&self, other: usize) -> Relationship {
        if other == self.position {
            return Relationship::SelfPlayer;
        }
        match self.landlord_position {
            Some(landlord) if landlord != self.position && other != landlord => Relationship::Ally,
            _ => Relationship::Enemy,
        }
    }

    pub fn next_position(&self, position: usize) -> Option<usize> {
        let index = self
            .positions
            .iter()
            .position(|candidate| *candidate == position)?;
        self.positions
            .get((index + 1) % self.positions.len())
            .copied()
    }
}

pub fn choose_bid(state: &LandlordLoopState, position: usize) -> u8 {
    AiObservation::from_state(state, position)
        .map(|observation| bidding::choose_bid(&observation))
        .unwrap_or(0)
}

pub fn choose_play(state: &LandlordLoopState, position: usize) -> Vec<i32> {
    AiObservation::from_state(state, position)
        .map(|observation| playing::choose_play(&observation))
        .unwrap_or_default()
}

pub fn hand_has_bomb(hand: &[i32]) -> bool {
    all_candidates(hand)
        .iter()
        .any(|candidate| matches!(candidate.combo.kind, ComboKind::Bomb | ComboKind::Rocket))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::LandlordPhase;
    use ws_common::CommonGameState;

    use crate::game_state::{LandlordLoopState, LandlordPlayRecord};

    use super::{AiObservation, Relationship, choose_play};

    pub(super) fn state_with_hands(hands: &[(usize, Vec<i32>)]) -> LandlordLoopState {
        let mut common = CommonGameState::new();
        for (position, _) in hands {
            common
                .players
                .insert(*position, (*position as u64 + 1, format!("P{position}")));
        }
        let mut state = LandlordLoopState::new(Arc::new(Mutex::new(common)));
        state.hands = hands.iter().cloned().collect();
        state
    }

    #[test]
    fn farmers_are_allies_and_landlord_is_enemy() {
        let mut state = state_with_hands(&[(0, vec![1]), (1, vec![2]), (2, vec![3])]);
        state.landlord_position = Some(0);
        let farmer = AiObservation::from_state(&state, 1).expect("farmer observation");
        assert_eq!(farmer.relationship_to(2), Relationship::Ally);
        assert_eq!(farmer.relationship_to(0), Relationship::Enemy);

        let landlord = AiObservation::from_state(&state, 0).expect("landlord observation");
        assert_eq!(landlord.relationship_to(1), Relationship::Enemy);
        assert_eq!(landlord.relationship_to(2), Relationship::Enemy);
    }

    #[test]
    fn observation_and_decision_ignore_hidden_opponent_cards() {
        let own_hand = vec![1, 14, 2, 15, 3, 16, 4];
        let mut left = state_with_hands(&[
            (0, own_hand.clone()),
            (1, vec![5, 18, 31]),
            (2, vec![6, 19]),
        ]);
        let mut right =
            state_with_hands(&[(0, own_hand), (1, vec![11, 24, 37]), (2, vec![53, 54])]);
        for state in [&mut left, &mut right] {
            state.phase = LandlordPhase::Play;
            state.current_position = 0;
            state.landlord_position = Some(0);
            state.hidden_cards = vec![7, 20, 33];
            state.play_history.push(LandlordPlayRecord {
                position: 2,
                cards: vec![8],
                benchmark: Vec::new(),
            });
        }

        let left_observation = AiObservation::from_state(&left, 0).expect("left observation");
        let right_observation = AiObservation::from_state(&right, 0).expect("right observation");

        assert_eq!(left_observation, right_observation);
        assert_eq!(choose_play(&left, 0), choose_play(&right, 0));
    }

    #[test]
    fn bomb_signal_is_visible_to_ai_farmers_only() {
        let mut state =
            state_with_hands(&[(0, vec![1, 2, 3]), (1, vec![4, 5, 6]), (2, vec![7, 8, 9])]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.ai_bomb_signal_used = true;
        state.ai_bomb_signal_position = Some(1);
        {
            let mut common = state.base.lock().unwrap();
            common.mark_ai_takeover_position(1);
            common.mark_ai_position(2);
        }

        assert_eq!(
            AiObservation::from_state(&state, 2)
                .expect("AI farmer observation")
                .ai_bomb_signal_position,
            Some(1)
        );
        assert_eq!(
            AiObservation::from_state(&state, 0)
                .expect("landlord observation")
                .ai_bomb_signal_position,
            None
        );
        assert_eq!(
            AiObservation::from_state(&state, 1)
                .expect("signaler observation")
                .ai_bomb_signal_position,
            Some(1)
        );

        state.base.lock().unwrap().clear_ai_takeover_position(1);
        assert_eq!(
            AiObservation::from_state(&state, 2)
                .expect("AI farmer observation after teammate resumes")
                .ai_bomb_signal_position,
            Some(1)
        );
        assert_eq!(
            AiObservation::from_state(&state, 1)
                .expect("human signaler observation")
                .ai_bomb_signal_position,
            None
        );

        state.base.lock().unwrap().ai_positions.remove(&2);
        assert_eq!(
            AiObservation::from_state(&state, 2)
                .expect("human farmer observation")
                .ai_bomb_signal_position,
            None
        );

        state.base.lock().unwrap().mark_ai_takeover_position(2);
        assert_eq!(
            AiObservation::from_state(&state, 2)
                .expect("AI takeover farmer observation")
                .ai_bomb_signal_position,
            Some(1)
        );
    }
}
