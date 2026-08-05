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
