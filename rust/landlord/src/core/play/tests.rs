use share_type_public::LandlordPhase;

use super::{
    Combo, ComboKind, PlayValidationContext, can_beat, card_rank, cards_in_hand, classify,
    hand_has_bomb_response, validate_play,
};

fn combo(kind: ComboKind, main_rank: u8, sequence_len: usize) -> Combo {
    Combo {
        kind,
        main_rank,
        sequence_len,
    }
}

#[test]
fn card_ranks_and_every_legal_combo_shape_are_classified() {
    assert_eq!(card_rank(1), 3);
    assert_eq!(card_rank(13), 15);
    assert_eq!(card_rank(53), 16);
    assert_eq!(card_rank(54), 17);

    let cases = [
        (&[53, 54][..], combo(ComboKind::Rocket, 17, 1)),
        (&[1, 14, 27, 40][..], combo(ComboKind::Bomb, 3, 1)),
        (&[1][..], combo(ComboKind::Single, 3, 1)),
        (&[1, 14][..], combo(ComboKind::Pair, 3, 1)),
        (&[1, 14, 27][..], combo(ComboKind::Triple, 3, 1)),
        (&[1, 14, 27, 2][..], combo(ComboKind::TripleSingle, 3, 1)),
        (&[1, 14, 27, 2, 15][..], combo(ComboKind::TriplePair, 3, 1)),
        (&[1, 2, 3, 4, 5][..], combo(ComboKind::Straight, 7, 5)),
        (
            &[1, 14, 2, 15, 3, 16][..],
            combo(ComboKind::StraightPairs, 5, 3),
        ),
        (&[1, 14, 27, 2, 15, 28][..], combo(ComboKind::Plane, 4, 2)),
        (
            &[1, 14, 27, 2, 15, 28, 3, 4][..],
            combo(ComboKind::PlaneWithSingles, 4, 2),
        ),
        (
            &[1, 14, 27, 2, 15, 28, 3, 16, 4, 17][..],
            combo(ComboKind::PlaneWithPairs, 4, 2),
        ),
        (
            &[1, 14, 27, 40, 2, 3][..],
            combo(ComboKind::FourWithTwoSingles, 3, 1),
        ),
        (
            &[1, 14, 27, 40, 2, 15, 3, 16][..],
            combo(ComboKind::FourWithTwoPairs, 3, 1),
        ),
    ];

    for (cards, expected) in cases {
        assert_eq!(classify(cards), Some(expected), "cards: {cards:?}");
    }
    assert_eq!(classify(&[]), None);
    assert_eq!(classify(&[55]), None);
    assert_eq!(classify(&[1, 14, 2, 15, 3]), None);
    assert_eq!(classify(&[11, 12, 13, 53, 54]), None);
}

#[test]
fn combo_comparison_and_bomb_availability_follow_landlord_precedence() {
    let single_three = combo(ComboKind::Single, 3, 1);
    let single_four = combo(ComboKind::Single, 4, 1);
    let pair_four = combo(ComboKind::Pair, 4, 1);
    let bomb_three = combo(ComboKind::Bomb, 3, 1);
    let bomb_four = combo(ComboKind::Bomb, 4, 1);
    let rocket = combo(ComboKind::Rocket, 17, 1);

    assert!(can_beat(&single_four, &single_three));
    assert!(!can_beat(&single_three, &single_four));
    assert!(!can_beat(&pair_four, &single_three));
    assert!(can_beat(&bomb_three, &pair_four));
    assert!(can_beat(&bomb_four, &bomb_three));
    assert!(!can_beat(&bomb_three, &bomb_four));
    assert!(!can_beat(&bomb_four, &rocket));
    assert!(can_beat(&rocket, &bomb_four));
    assert!(!can_beat(&rocket, &rocket));
    assert!(!can_beat(&single_four, &bomb_three));

    assert!(hand_has_bomb_response(&[1, 14, 27, 40], &single_three));
    assert!(!hand_has_bomb_response(&[1, 14, 27, 40], &bomb_four));
    assert!(hand_has_bomb_response(&[53, 54], &bomb_four));
    assert!(!hand_has_bomb_response(&[53, 54], &rocket));
}

#[test]
fn malformed_combo_shapes_do_not_match_partial_patterns() {
    assert_eq!(classify(&[1, 14, 27, 2, 3]), None);
    assert_eq!(classify(&[1, 14, 27, 2, 15, 3, 4]), None);
    assert_eq!(classify(&[1, 14, 27, 2, 15, 3, 16, 4, 5]), None);
    assert_eq!(classify(&[1, 3, 5, 7, 9]), None);
    assert_eq!(classify(&[1, 14, 27, 40, 2, 15, 3, 4]), None);
    assert_eq!(
        classify(&[1, 14, 27, 40, 2, 15]),
        None,
        "四带二单不能使用同牌点的一对翼"
    );
}

#[test]
fn hand_membership_and_play_validation_reject_illegal_turns_and_passes() {
    assert!(cards_in_hand(&[1, 14], &[1, 14, 2]));
    assert!(!cards_in_hand(&[1, 1], &[1]));
    assert!(cards_in_hand(&[55], &[55]));

    let hand = [1, 14, 2, 15, 3, 16, 4, 17, 53, 54];
    let leading = PlayValidationContext {
        phase: LandlordPhase::Play,
        current_position: 0,
        hand: Some(&hand),
        last_play_position: 2,
        last_play: &[],
    };
    assert!(validate_play(leading, 0, &[1]));
    assert!(!validate_play(leading, 0, &[]));
    assert!(!validate_play(leading, 1, &[1]));
    assert!(!validate_play(
        PlayValidationContext {
            phase: LandlordPhase::CallLandlord,
            ..leading
        },
        0,
        &[1],
    ));
    assert!(!validate_play(
        PlayValidationContext {
            hand: None,
            ..leading
        },
        0,
        &[1],
    ));
    assert!(!validate_play(leading, 0, &[55]));
    assert!(!validate_play(leading, 0, &[40]));
    assert!(!validate_play(leading, 0, &[1, 14, 2, 15, 3]));

    let following = PlayValidationContext {
        last_play_position: 1,
        last_play: &[1],
        ..leading
    };
    assert!(validate_play(following, 0, &[]));
    assert!(validate_play(following, 0, &[2]));
    assert!(!validate_play(following, 0, &[1]));
    assert!(validate_play(following, 0, &[53, 54]));
    assert!(!validate_play(
        PlayValidationContext {
            last_play: &[55],
            ..following
        },
        0,
        &[2],
    ));
    assert!(!validate_play(
        PlayValidationContext {
            last_play: &[1],
            ..following
        },
        0,
        &[1, 14, 2, 15, 3],
    ));
    assert!(!validate_play(
        PlayValidationContext {
            last_play_position: 0,
            ..following
        },
        0,
        &[],
    ));
}
