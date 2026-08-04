use std::sync::{Arc, Mutex};

use share_type_public::{UpgradePhase, UpgradeSuit};
use ws_common::CommonGameState;

use super::*;

fn rules(deck_count: u8) -> UpgradeRules {
    UpgradeRules {
        deck_count: UpgradeDeckCount::new(deck_count).unwrap(),
        target_rank: Rank::Three,
        final_target_rank: Rank::Ace,
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        trump_suit: None,
    }
}

#[test]
fn deal_has_four_even_hands_and_private_bottom() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.deal_new_round(rules(4)).unwrap();

    assert_eq!(state.phase, UpgradePhase::Bury);
    assert_eq!(state.bottom_cards.len(), 8);
    assert_eq!(state.hand_count(), 52);
    assert_eq!(state.hands[&0].len(), 60);
    assert!((1..4).all(|position| state.hands[&position].len() == 52));
    assert_eq!(state.dealt_count, 216);
}

#[test]
fn bury_then_select_trump_enters_play() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.deal_new_round(rules(3)).unwrap();
    let bottom = state.bottom_cards.clone();
    state.bury_bottom(0, bottom).unwrap();
    assert_eq!(state.phase, UpgradePhase::Play);
    assert!(state.select_trump(0, UpgradeSuit::HEART).is_err());

    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut later_round = UpgradeGameState::from_common(common);
    later_round.round_index = 1;
    later_round.deal_new_round(rules(3)).unwrap();
    let bottom = later_round.bottom_cards.clone();
    later_round.bury_bottom(0, bottom).unwrap();
    assert_eq!(later_round.phase, UpgradePhase::Bury);
    later_round.select_trump(0, UpgradeSuit::HEART).unwrap();
    assert_eq!(later_round.rules.trump_suit, Some(Suit::Heart));
}

fn playing_state(hands: HashMap<usize, Vec<i32>>) -> UpgradeGameState {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    for position in 0..4 {
        common
            .lock()
            .unwrap()
            .add_player(position, position as u64 + 1, &format!("p{position}"));
    }
    let mut state = UpgradeGameState::from_common(common);
    state.phase = UpgradePhase::Play;
    state.rules = UpgradeRules {
        trump_suit: Some(Suit::Heart),
        ..rules(3)
    };
    state.hands = hands;
    state.current_position = 0;
    state.bottom_cards.clear();
    state
}

#[test]
fn follower_must_use_the_led_group_when_available() {
    let mut state = playing_state(HashMap::from([
        (0, vec![]),
        (1, vec![5, 18]),
        (2, vec![6]),
        (3, vec![7]),
    ]));
    state.current_trick.push(WsUpgradePlayedCards {
        position: 0,
        name: "p0".into(),
        cards: vec![4],
    });
    state.current_position = 1;

    assert!(matches!(
        state.play_cards(1, vec![18]),
        Err("illegal follow")
    ));
    assert!(state.play_cards(1, vec![5]).is_ok());
}

#[test]
fn higher_triple_forces_only_the_triple_component_back() {
    let attempted = vec![2, 102, 202, 12, 112, 13];
    let mut state = playing_state(HashMap::from([
        (0, attempted.clone()),
        (1, vec![3, 103, 203, 20, 21, 22]),
        (2, vec![30; 6]),
        (3, vec![43; 6]),
    ]));
    state.rules.target_rank = Rank::Two;

    let result = state.play_cards(0, attempted).unwrap();

    assert_eq!(result.played_cards, vec![2, 102, 202]);
    assert_eq!(result.failed_throw.unwrap().attempted_cards.len(), 6);
    assert_eq!(state.hands[&0], vec![12, 112, 13]);
}

#[test]
fn four_single_plays_finish_the_last_trick_and_settle() {
    let mut state = playing_state(HashMap::from([
        (0, vec![4]),
        (1, vec![13]),
        (2, vec![5]),
        (3, vec![6]),
    ]));

    for (position, card) in [(0, 4), (1, 13), (2, 5), (3, 6)] {
        state.play_cards(position, vec![card]).unwrap();
    }

    assert_eq!(state.phase, UpgradePhase::Settlement);
    assert_eq!(state.last_trick_winner, Some(1));
    assert_eq!(state.collected_scores.get(&1), Some(&5));
    let settlement = state.settlement_event();
    assert_eq!(settlement.score, 5);
    assert_eq!(settlement.winner_positions, vec![0, 2]);
}

#[test]
fn settlement_can_start_the_next_round_and_raise_target_rank() {
    let mut state = playing_state(HashMap::from([
        (0, vec![4]),
        (1, vec![13]),
        (2, vec![5]),
        (3, vec![6]),
    ]));
    for (position, card) in [(0, 4), (1, 13), (2, 5), (3, 6)] {
        state.play_cards(position, vec![card]).unwrap();
    }

    assert!(state.advance_after_settlement().unwrap());
    assert_eq!(state.phase, UpgradePhase::Bury);
    assert_eq!(state.round_index, 1);
    assert_eq!(state.rules.target_rank, Rank::Five);
    assert_eq!(state.rules.trump_suit, None);
}
