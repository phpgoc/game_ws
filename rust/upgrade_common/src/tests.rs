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
fn ordinary_twos_stay_in_their_natural_suit_outside_the_main_suit() {
    for target in [Rank::Three, Rank::Five, Rank::Ace] {
        let rules = (target, Some(Suit::Heart));
        assert!(!card_is_trump(Card::try_from(1).unwrap(), rules.0, rules.1));
        assert!(card_is_trump(Card::try_from(14).unwrap(), rules.0, rules.1));
        assert!(!card_is_trump(
            Card::try_from(27).unwrap(),
            rules.0,
            rules.1
        ));
        assert!(!card_is_trump(
            Card::try_from(40).unwrap(),
            rules.0,
            rules.1
        ));
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
fn main_suit_two_beats_plain_cards_at_every_runtime_level() {
    for target in [Rank::Three, Rank::Five, Rank::Ace] {
        let highest_plain = if target == Rank::Ace { 25 } else { 26 };
        let highest_plain_position = trump_order_position(
            Card::try_from(highest_plain).unwrap(),
            target,
            Some(Suit::Heart),
        )
        .expect("highest main-suit plain card is in the trump suit");
        assert!(
            trump_order_position(Card::try_from(14).unwrap(), target, Some(Suit::Heart))
                .expect("main-suit two is trump")
                > highest_plain_position,
            "main-suit two must beat the highest plain card at target {target:?}",
        );
    }
}

#[test]
fn trump_order_keeps_every_special_boundary_consecutive() {
    let target = Rank::Three;
    let trump = Some(Suit::Heart);
    let ordered = [
        26, // 主 A
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
fn ace_level_keeps_the_highest_remaining_trump_next_to_main_suit_two() {
    let target = Rank::Ace;
    let trump = Some(Suit::Heart);
    let trump_king = trump_order_position(Card::try_from(25).unwrap(), target, trump).unwrap();
    let main_suit_two = trump_order_position(Card::try_from(14).unwrap(), target, trump).unwrap();

    assert_eq!(main_suit_two, trump_king + 1);
}

#[test]
fn compact_plain_sequence_closes_only_the_level_gap() {
    assert_eq!(compact_plain_rank_position(Rank::Four, Rank::Five), Some(1));
    assert_eq!(compact_plain_rank_position(Rank::Six, Rank::Five), Some(2));
    assert_eq!(compact_plain_rank_position(Rank::Five, Rank::Five), None);
    assert_eq!(compact_plain_rank_position(Rank::Two, Rank::Five), None);
}
