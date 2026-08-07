use std::sync::{Arc, Mutex};

use share_type_public::{UpgradePhase, UpgradeSuit};
use ws_common::CommonGameState;

use super::*;

fn rules(deck_count: u8) -> UpgradeRules {
    UpgradeRules {
        deck_count: UpgradeDeckCount::new(deck_count).unwrap(),
        target_rank: Rank::Three,
        final_target_rank: Rank::Ace,
        removed_rank_count: 0,
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        trump_suit: None,
    }
}

fn finish_deal(state: &mut UpgradeGameState) {
    while state.phase == UpgradePhase::Deal {
        state.deal_next_card().expect("next upgrade deal card");
    }
}

#[test]
fn deal_has_four_even_hands_and_private_bottom() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.deal_new_round(rules(4)).unwrap();

    assert_eq!(state.phase, UpgradePhase::Deal);
    assert_eq!(state.total_deal_count, 208);
    assert!(state.hands.values().all(Vec::is_empty));
    finish_deal(&mut state);
    assert_eq!(state.phase, UpgradePhase::Bury);
    assert_eq!(state.bottom_cards.len(), 8);
    assert_eq!(state.hand_count(), 52);
    assert_eq!(state.hands[&0].len(), 60);
    assert!((1..4).all(|position| state.hands[&position].len() == 52));
    assert_eq!(state.dealt_count, 208);
    assert_eq!(
        state.declaration.as_ref().map(|item| item.target_rank),
        Some(UpgradeRank::THREE)
    );
    assert!(state.rules.trump_suit.is_some());
}

#[test]
fn removed_ranks_shrink_the_deck_and_are_skipped_as_levels() {
    let deck_count = UpgradeDeckCount::new(3).unwrap();
    let deck = build_upgrade_deck_with_removed_ranks(deck_count, 4);
    assert_eq!(deck.len(), 3 * (54 - 4 * 4));
    assert!(deck.iter().all(|card| {
        let rank = Card::try_from(*card).unwrap().rank();
        ![Rank::Three, Rank::Four, Rank::Six, Rank::Seven].contains(&rank)
    }));
    assert_eq!(first_upgrade_rank(4, Rank::Ace), Rank::Five);

    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    let mut compact_rules = rules(3);
    compact_rules.removed_rank_count = 4;
    state.deal_new_round(compact_rules).unwrap();
    assert_eq!(state.rules.target_rank, Rank::Five);
    assert_eq!(state.snapshot().removed_rank_count, 4);
    assert_eq!(state.total_deal_count, 104);

    state.phase = UpgradePhase::Settlement;
    state.team_target_ranks = [Rank::Five; 2];
    assert_eq!(state.next_target_rank(), Some(Rank::Ten));
}

#[test]
fn first_round_declarations_reveal_three_and_only_stronger_copies_can_take_over() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    for position in 0..4 {
        common
            .lock()
            .unwrap()
            .add_player(position, position as u64 + 1, &format!("p{position}"));
    }
    let mut state = UpgradeGameState::from_common(common);
    state.phase = UpgradePhase::Deal;
    state.rules = rules(3);
    state.hands.insert(0, vec![2]);
    state.hands.insert(1, vec![15, 115]);

    let first = state.declare_trump(0, vec![2]).unwrap();
    assert_eq!(first.target_rank, UpgradeRank::THREE);
    assert_eq!(first.strength, 1);
    assert!(state.declare_trump(1, vec![15]).is_err());

    let stronger = state.declare_trump(1, vec![15, 115]).unwrap();
    assert_eq!(stronger.target_rank, UpgradeRank::THREE);
    assert_eq!(stronger.strength, 2);
    assert_eq!(state.dealer_position, 1);
}

#[test]
fn first_round_can_reveal_a_level_card_from_the_bottom() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.phase = UpgradePhase::Deal;
    state.rules = rules(3);
    state.bottom_cards = vec![2];
    state.deal_queue.push_back((1, 4));

    let (_, _, finished, declaration) = state.deal_next_card().expect("deal the final card");

    assert!(finished);
    let declaration = declaration.expect("bottom fallback declaration");
    assert_eq!(declaration.position, 0);
    assert_eq!(declaration.cards, vec![2]);
    assert_eq!(state.rules.trump_suit, Some(Suit::Spade));
    assert_eq!(state.phase, UpgradePhase::Bury);
}

#[test]
fn later_round_selects_trump_then_buries_in_one_operation_window() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.deal_new_round(rules(3)).unwrap();
    finish_deal(&mut state);
    let bottom = state.bottom_cards.clone();
    // 首局兜底亮主的庄家由实际亮到级牌的座位决定，不能假设仍是 0 号位。
    let first_dealer = state.dealer_position;
    state.bury_bottom(first_dealer, bottom).unwrap();
    assert_eq!(state.phase, UpgradePhase::Play);
    assert!(
        state
            .select_trump(first_dealer, UpgradeSuit::HEART)
            .is_err()
    );

    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut later_round = UpgradeGameState::from_common(common);
    later_round.round_index = 1;
    later_round.deal_new_round(rules(3)).unwrap();
    finish_deal(&mut later_round);
    let bottom = later_round.bottom_cards.clone();
    assert!(later_round.bury_bottom(0, bottom).is_err());
    assert_eq!(later_round.phase, UpgradePhase::Bury);

    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut later_round = UpgradeGameState::from_common(common);
    later_round.round_index = 1;
    later_round.deal_new_round(rules(3)).unwrap();
    finish_deal(&mut later_round);
    let bottom = later_round.bottom_cards.clone();
    later_round.select_trump(0, UpgradeSuit::HEART).unwrap();
    assert_eq!(later_round.rules.trump_suit, Some(Suit::Heart));
    assert_eq!(later_round.phase, UpgradePhase::Bury);
    later_round.bury_bottom(0, bottom).unwrap();
    assert_eq!(later_round.phase, UpgradePhase::Play);
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
fn failed_throw_uses_the_lowest_ranked_beatable_component_across_all_opponents() {
    let attempted = vec![2, 102, 202, 5, 105, 13];
    let mut state = playing_state(HashMap::from([
        (0, attempted.clone()),
        (1, vec![3, 103, 203]),
        (2, vec![6, 106]),
        (3, vec![20; 6]),
    ]));
    state.rules.target_rank = Rank::Two;

    let result = state.play_cards(0, attempted).unwrap();

    assert_eq!(result.played_cards, vec![2, 102, 202]);
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
    assert_eq!(settlement.next_target_rank, Some(UpgradeRank::FIVE));
    assert!(!settlement.match_finished);
}

#[test]
fn level_and_main_level_cards_beat_an_ordinary_trump_ace() {
    let mut state = playing_state(HashMap::from([
        (0, vec![26]),
        (1, vec![2]),
        (2, vec![15]),
        (3, vec![14]),
    ]));
    state.rules.target_rank = Rank::Three;

    for (position, card) in [(0, 26), (1, 2), (2, 15), (3, 14)] {
        state.play_cards(position, vec![card]).unwrap();
    }

    assert_eq!(state.last_trick_winner, Some(2));
}

#[test]
fn pair_on_the_last_trick_doubles_the_bottom_score() {
    let mut state = playing_state(HashMap::from([
        (0, vec![3, 103]),
        (1, vec![5, 105]),
        (2, vec![6, 106]),
        (3, vec![7, 107]),
    ]));
    state.bottom_cards = vec![9];

    for (position, cards) in [
        (0, vec![3, 103]),
        (1, vec![5, 105]),
        (2, vec![6, 106]),
        (3, vec![7, 107]),
    ] {
        state.play_cards(position, cards).unwrap();
    }

    assert_eq!(state.last_trick_winner, Some(3));
    assert_eq!(state.bottom_multiplier, 2);
    assert_eq!(state.collected_scores.get(&3), Some(&20));
}

#[test]
fn successful_long_throw_uses_only_its_largest_component_for_bottom_score() {
    let lead = vec![13, 113, 213, 11, 111, 10];
    let mut state = playing_state(HashMap::from([
        (0, lead.clone()),
        (1, vec![2, 3, 5, 6, 7, 8]),
        (2, vec![102, 103, 105, 106, 107, 108]),
        (3, vec![202, 203, 205, 206, 207, 208]),
    ]));
    state.bottom_cards = vec![9];

    for (position, cards) in [
        (0, lead),
        (1, vec![2, 3, 5, 6, 7, 8]),
        (2, vec![102, 103, 105, 106, 107, 108]),
        (3, vec![202, 203, 205, 206, 207, 208]),
    ] {
        state.play_cards(position, cards).unwrap();
    }

    assert_eq!(state.last_trick_winner, Some(0));
    assert_eq!(state.bottom_multiplier, 3);
    assert_eq!(state.collected_scores.get(&0), Some(&30));
}

#[test]
fn one_trump_pair_and_four_singles_cannot_beat_a_three_pair_throw() {
    let mut state = playing_state(HashMap::from([
        (0, vec![3, 103, 5, 105, 6, 106]),
        (1, vec![26, 126, 25, 24, 23, 22]),
        (2, vec![27, 29, 30, 31, 32, 33]),
        (3, vec![40, 42, 43, 44, 45, 46]),
    ]));

    for (position, cards) in [
        (0, vec![3, 103, 5, 105, 6, 106]),
        (1, vec![26, 126, 25, 24, 23, 22]),
        (2, vec![27, 29, 30, 31, 32, 33]),
        (3, vec![40, 42, 43, 44, 45, 46]),
    ] {
        state.play_cards(position, cards).unwrap();
    }

    assert_eq!(state.last_trick_winner, Some(0));
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
    assert_eq!(state.phase, UpgradePhase::Deal);
    assert_eq!(state.round_index, 1);
    assert_eq!(state.rules.target_rank, Rank::Five);
    assert_eq!(state.rules.trump_suit, None);
}

#[test]
fn settlement_caps_multi_level_progress_and_finishes_on_ace() {
    let mut state = playing_state(HashMap::new());
    state.phase = UpgradePhase::Settlement;
    state.rules.target_rank = Rank::Queen;
    state.team_target_ranks = [Rank::Queen; 2];

    let settlement = state.settlement_event();
    assert_eq!(settlement.next_target_rank, Some(UpgradeRank::A));
    assert!(!settlement.match_finished);

    state.rules.target_rank = Rank::Ace;
    state.team_target_ranks = [Rank::Ace; 2];
    let settlement = state.settlement_event();
    assert_eq!(settlement.next_target_rank, None);
    assert!(settlement.match_finished);
    assert!(!state.advance_after_settlement().unwrap());
}

#[test]
fn attacking_team_advances_from_its_own_level_when_it_takes_over() {
    let mut state = playing_state(HashMap::new());
    state.phase = UpgradePhase::Settlement;
    state.dealer_position = 0;
    state.rules.target_rank = Rank::Five;
    state.team_target_ranks = [Rank::Five, Rank::Three];
    state.collected_scores.insert(1, 80);

    let settlement = state.settlement_event();
    assert_eq!(settlement.winner_positions, vec![1, 3]);
    assert_eq!(settlement.next_target_rank, Some(UpgradeRank::FOUR));
    assert_eq!(
        settlement.team_target_ranks,
        vec![UpgradeRank::FIVE, UpgradeRank::FOUR]
    );

    assert!(state.advance_after_settlement().unwrap());
    assert_eq!(state.dealer_position, 1);
    assert_eq!(state.rules.target_rank, Rank::Four);
    assert_eq!(state.team_target_ranks, [Rank::Five, Rank::Four]);
}

#[test]
fn timeout_actions_keep_the_game_moving_without_a_human_request() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.deal_new_round(rules(3)).unwrap();
    finish_deal(&mut state);
    state.set_turn_countdown(0);
    assert!(state.timeout_bury().unwrap());
    assert_eq!(state.phase, UpgradePhase::Play);

    state.hands = HashMap::from([(0, vec![4]), (1, vec![5]), (2, vec![6]), (3, vec![7])]);
    state.current_position = 0;
    state.set_turn_countdown(0);
    for _ in 0..4 {
        assert!(state.timeout_play().unwrap().is_some());
        if state.phase == UpgradePhase::Settlement {
            break;
        }
        state.set_turn_countdown(0);
    }
    assert_eq!(state.phase, UpgradePhase::Settlement);
}

#[test]
fn timeout_follow_preserves_an_available_pair() {
    let mut state = playing_state(HashMap::from([
        (0, vec![]),
        (1, vec![5, 6, 105]),
        (2, vec![7, 107, 8]),
        (3, vec![9, 109, 10]),
    ]));
    state.current_trick.push(WsUpgradePlayedCards {
        position: 0,
        name: "p0".into(),
        cards: vec![4, 104],
    });
    state.current_position = 1;

    let played = state
        .timeout_play()
        .expect("timeout follow should stay legal")
        .expect("timeout should play a pair follow");

    assert_eq!(played.played_cards, vec![5, 105]);
    assert_eq!(state.hands[&1], vec![6]);
}
