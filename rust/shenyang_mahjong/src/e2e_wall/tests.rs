use super::*;
use crate::rules::{ShenyangMahjongWinContext, satisfies_shenyang_win_with_context};

#[test]
fn fixture_is_a_complete_wall_with_two_legal_closed_rob_gang_waits() {
    let mut wall = wall_for_fixture(Some("dual-rob-gang"), 0, 0).unwrap();
    assert_eq!(wall.len(), 136);
    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        assert_eq!(wall.iter().filter(|&&value| value == tile).count(), 4);
    }
    let mut hands = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for _ in 0..13 {
        for hand in &mut hands {
            hand.push(wall.pop().unwrap());
        }
    }
    for (hand, expected) in hands.iter().zip(DUAL_ROB_GANG_HANDS) {
        assert_eq!(hand, &expected);
    }
    let draws = (0..7).map(|_| wall.pop().unwrap()).collect::<Vec<_>>();
    assert_eq!(draws, [37, 34, 32, 33, 34, 32, 3]);
    for position in [0, 1] {
        let mut winning_hand = DUAL_ROB_GANG_HANDS[position].to_vec();
        assert!(!winning_hand.contains(&3));
        winning_hand.push(3);
        assert!(satisfies_shenyang_win_with_context(
            &winning_hand,
            &[],
            ShenyangMahjongWinContext::from_allow_first_chi(false),
        ));
    }
}

#[test]
fn fixture_only_applies_to_the_explicit_first_round_dealer() {
    assert!(wall_for_fixture(None, 0, 0).is_none());
    assert!(wall_for_fixture(Some("unknown"), 0, 0).is_none());
    assert!(wall_for_fixture(Some("dual-rob-gang"), 1, 0).is_none());
    assert!(wall_for_fixture(Some("dual-rob-gang"), 0, 1).is_none());
}
