use super::*;

fn cards(encoded: &[i32]) -> Vec<Card> {
    encoded
        .iter()
        .copied()
        .map(Card::try_from)
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn decodes_existing_multi_deck_card_ids() {
    let card = Card::try_from(502).unwrap();
    assert_eq!(card.deck_index(), 5);
    assert_eq!(card.identity(), 2);
    assert_eq!(card.rank(), Rank::Three);
    assert_eq!(card.suit(), Some(Suit::Spade));
    assert_eq!(i32::from(card), 502);
}

#[test]
fn rejects_cards_outside_six_decks_or_standard_identity() {
    assert_eq!(Card::try_from(0), Err(CardDecodeError::NonPositive(0)));
    assert_eq!(
        Card::try_from(55),
        Err(CardDecodeError::InvalidIdentity(55))
    );
    assert_eq!(Card::try_from(601), Err(CardDecodeError::TooManyDecks(6)));
}

#[test]
fn reports_only_the_largest_identity_group() {
    // 三张 3、两张 K 和一张 A。这个原语只报告最长的三张，不把整甩算成六张。
    let throw = cards(&[2, 102, 202, 12, 112, 13]);
    assert_eq!(largest_identity_group_size(&throw), 3);

    let three_pairs = cards(&[5, 105, 6, 106, 7, 107]);
    assert_eq!(largest_identity_group_size(&three_pairs), 2);
}

#[test]
fn score_progression_is_configurable_and_handles_standard_boundaries() {
    let progression = ScoreProgression::new(80, 40, 1).unwrap();

    assert_eq!(progression.outcome(0), ScoreOutcome::defending(3));
    assert_eq!(progression.outcome(39), ScoreOutcome::defending(2));
    assert_eq!(progression.outcome(40), ScoreOutcome::defending(1));
    assert_eq!(progression.outcome(79), ScoreOutcome::defending(1));
    assert_eq!(progression.outcome(80), ScoreOutcome::attacking(1));
    assert_eq!(progression.outcome(120), ScoreOutcome::attacking(2));
}

#[test]
fn score_progression_rejects_non_positive_configuration() {
    assert!(ScoreProgression::new(0, 40, 1).is_err());
    assert!(ScoreProgression::new(80, 0, 1).is_err());
}

#[test]
fn standard_four_player_dealer_rotates_to_partner_or_next_seat() {
    assert_eq!(next_four_player_dealer(0, ScoreSide::Defending), 2);
    assert_eq!(next_four_player_dealer(2, ScoreSide::Defending), 0);
    assert_eq!(next_four_player_dealer(0, ScoreSide::Attacking), 1);
    assert_eq!(next_four_player_dealer(3, ScoreSide::Attacking), 0);
}

#[test]
fn level_path_supports_standard_and_compact_games() {
    assert_eq!(
        level_rank_path(Rank::Seven, &[]),
        vec![Rank::Three, Rank::Four, Rank::Five, Rank::Six, Rank::Seven,]
    );
    assert_eq!(
        level_rank_path(Rank::Seven, &[Rank::Three, Rank::Four, Rank::Six]),
        vec![Rank::Five, Rank::Seven]
    );
}

#[test]
fn level_advance_caps_at_the_final_rank_and_finishes_there() {
    assert_eq!(
        next_level_rank(Rank::Queen, Rank::Ace, &[], 3),
        Some(Rank::Ace)
    );
    assert_eq!(next_level_rank(Rank::Ace, Rank::Ace, &[], 1), None);
    assert_eq!(
        next_level_rank(Rank::Five, Rank::Nine, &[Rank::Six, Rank::Seven], 2,),
        Some(Rank::Nine)
    );
}

#[test]
fn permanent_twos_and_level_cards_share_the_trump_group() {
    for target in [Rank::Three, Rank::Five, Rank::Ace] {
        let rules = (target, Some(Suit::Heart));
        for encoded in [1, 14, 27, 40] {
            assert!(card_is_trump(
                Card::try_from(encoded).unwrap(),
                rules.0,
                rules.1
            ));
        }
    }
    let rules = (Rank::Five, Some(Suit::Heart));
    for encoded in [4, 17, 30, 43, 53, 54] {
        assert!(card_is_trump(
            Card::try_from(encoded).unwrap(),
            rules.0,
            rules.1
        ));
    }
    assert!(card_is_trump(Card::try_from(26).unwrap(), rules.0, rules.1));
    assert!(!card_is_trump(
        Card::try_from(13).unwrap(),
        rules.0,
        rules.1
    ));
}

#[test]
fn off_suit_twos_tie_below_the_trump_suit_two_at_every_runtime_level() {
    for target in [Rank::Three, Rank::Five, Rank::Ace] {
        let off_suit_positions = [1, 27, 40].map(|encoded| {
            trump_order_position(Card::try_from(encoded).unwrap(), target, Some(Suit::Heart))
                .expect("every off-suit two is permanent trump")
        });
        assert!(
            off_suit_positions
                .windows(2)
                .all(|positions| positions[0] == positions[1]),
            "off-suit twos must tie at target {target:?}",
        );

        let trump_two =
            trump_order_position(Card::try_from(14).unwrap(), target, Some(Suit::Heart))
                .expect("trump-suit two is permanent trump");
        assert_eq!(
            trump_two,
            off_suit_positions[0] + 1,
            "trump-suit two must sit immediately above every off-suit two at target {target:?}",
        );
    }
}

#[test]
fn trump_order_keeps_every_special_boundary_consecutive() {
    let target = Rank::Three;
    let trump = Some(Suit::Heart);
    let ordered = [
        26, // 主 A
        1,  // 副 2
        14, // 主 2
        2,  // 副级
        15, // 主级
        53, // 小王
        54, // 大王
    ];
    let positions = ordered.map(|encoded| {
        trump_order_position(Card::try_from(encoded).unwrap(), target, trump).unwrap()
    });
    assert!(positions.windows(2).all(|pair| pair[1] == pair[0] + 1));
}

#[test]
fn compact_plain_sequence_closes_only_the_level_gap() {
    assert_eq!(compact_plain_rank_position(Rank::Four, Rank::Five), Some(1));
    assert_eq!(compact_plain_rank_position(Rank::Six, Rank::Five), Some(2));
    assert_eq!(compact_plain_rank_position(Rank::Five, Rank::Five), None);
    assert_eq!(compact_plain_rank_position(Rank::Two, Rank::Five), None);
}
