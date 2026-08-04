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
