use super::*;
use share_type_public::TractorRank;

#[test]
fn ace_pair_and_queen_pair_form_throw_with_weak_pair_first() {
    let rules = rules(TractorRank::TWO);
    let cards = vec![13, 113, 11, 111];
    assert_eq!(
        classify(&cards, &rules).map(|combo| combo.kind),
        Some(ComboKind::Throw { cards: 4, pairs: 2 })
    );
    let components = throw_components(&cards, &rules).expect("throw components");
    assert_eq!(components, vec![vec![11, 111], vec![13, 113]]);
}

#[test]
fn declared_trump_suit_cards_beat_plain_cards() {
    let mut rules = rules(TractorRank::TWO);
    rules.trump_suit = Some(share_type_public::TractorSuit::HEART);
    let trick = [
        played(0, vec![13]), // spade A leads
        played(1, vec![15]), // heart 3 is trump and ruffs
    ];
    assert_eq!(trick_winner(&trick, &rules), Some(1));
}

#[test]
fn level_cards_rank_above_ordinary_trump_and_below_jokers() {
    let mut rules = rules(TractorRank::THREE);
    rules.trump_suit = Some(share_type_public::TractorSuit::HEART);
    assert!(is_trump_card(14, &rules));
    let ordered_cards = [
        26, // heart A: highest ordinary trump
        14, // heart 2: main-suit permanent trump
        2,  // spade 3: off-suit level card
        15, // heart 3: main level card
        53, // small joker
        54, // big joker
    ];
    let strengths = ordered_cards.map(|card| tractor_card_value(card, &rules, None));
    assert!(
        strengths.windows(2).all(|pair| pair[0] < pair[1]),
        "unexpected trump order: {strengths:?}"
    );

    let trick = [
        played(0, vec![26]),
        played(1, vec![2]),
        played(2, vec![15]),
        played(3, vec![53]),
    ];
    assert_eq!(trick_winner(&trick, &rules), Some(3));
}

#[test]
fn enumerate_leads_finds_pairs_and_tractors() {
    let rules = rules(TractorRank::TWO);
    let hand = vec![2, 102, 3, 103, 20];
    let leads = enumerate_leads(&hand, &rules);
    let has_tractor = leads.iter().any(|cards| {
        matches!(
            classify(cards, &rules).map(|c| c.kind),
            Some(ComboKind::Tractor(2))
        )
    });
    let has_pair = leads
        .iter()
        .any(|cards| classify(cards, &rules).map(|c| c.kind) == Some(ComboKind::Pair));
    assert!(has_tractor);
    assert!(has_pair);
    assert!(leads.iter().any(|cards| cards == &vec![20]));
}

#[test]
fn enumerate_leads_keeps_pair_single_and_multi_deck_pair_throws() {
    let rules = rules(TractorRank::TWO);
    let pair_single = enumerate_leads(&[11, 111, 13], &rules);
    assert!(pair_single.iter().any(|cards| {
        cards == &vec![11, 111, 13]
            && classify(cards, &rules).map(|combo| combo.kind)
                == Some(ComboKind::Throw { cards: 3, pairs: 1 })
    }));

    let mut four_deck_rules = rules.clone();
    four_deck_rules.deck_count = 4;
    let two_identical_pairs = enumerate_leads(&[2, 102, 202, 302], &four_deck_rules);
    assert!(two_identical_pairs.iter().any(|cards| {
        cards == &vec![2, 102, 202, 302]
            && classify(cards, &four_deck_rules).map(|combo| combo.kind)
                == Some(ComboKind::Throw { cards: 4, pairs: 2 })
    }));
}

#[test]
fn multi_deck_duplicate_pair_throw_follows_stay_legal() {
    let mut rules = rules(TractorRank::TWO);
    rules.deck_count = 4;
    let lead = classify(&[2, 102, 202, 302], &rules).expect("two-pair throw");
    let hand = vec![3, 103, 203, 303];
    let candidates = enumerate_follows(&hand, &lead, &rules);

    assert!(candidates.contains(&hand));
    assert!(
        candidates
            .iter()
            .all(|cards| follow_is_legal(&hand, cards, &lead, &rules))
    );
}

#[test]
fn follow_candidates_include_point_avoiding_single_combinations() {
    let rules = rules(TractorRank::TWO);
    let lead = classify(&[8, 108], &rules).expect("pair lead");
    let candidates = enumerate_follows(&[4, 5, 6], &lead, &rules);

    assert!(candidates.contains(&vec![4, 5]));
    assert!(candidates.contains(&vec![5, 6]));
    assert!(
        candidates
            .iter()
            .all(|cards| follow_is_legal(&[4, 5, 6], cards, &lead, &rules))
    );
}

#[test]
fn follow_candidates_merge_equivalent_physical_copies() {
    let mut rules = rules(TractorRank::TWO);
    rules.deck_count = 3;
    let lead = classify(&[8, 108], &rules).expect("pair lead");
    let hand = vec![4, 104, 204];
    let candidates = enumerate_follows(&hand, &lead, &rules);

    assert_eq!(candidates.len(), 1);
    assert!(follow_is_legal(&hand, &candidates[0], &lead, &rules));
}

#[test]
fn bounded_follow_candidates_keep_the_forced_reply_without_expanding_a_wide_subset() {
    let mut rules = rules(TractorRank::TWO);
    rules.deck_count = 3;
    let lead = classify(&[15, 115, 16, 116, 17, 117], &rules).expect("three-pair tractor");
    let hand = (15..=26).collect::<Vec<_>>();

    let full = enumerate_follows(&hand, &lead, &rules);
    let bounded = enumerate_follows_with_subset_limit(&hand, &lead, &rules, 512);

    assert_eq!(full.len(), 924);
    assert_eq!(bounded.len(), 1);
    assert!(follow_is_legal(&hand, &bounded[0], &lead, &rules));
}

#[test]
fn forced_follow_is_always_legal() {
    let rules = rules(TractorRank::TWO);
    let lead = classify(&[2, 102], &rules).unwrap();
    let hand = vec![3, 103, 20, 21];
    let follow = forced_follow(&hand, &lead, &rules).expect("forced follow");
    assert!(follow_is_legal(&hand, &follow, &lead, &rules));
    // Must reuse the held pair.
    assert_eq!(follow, vec![3, 103]);
}

#[test]
fn forced_tractor_follow_uses_multiple_pairs_of_one_identity() {
    let rules = rules(TractorRank::TWO);
    let lead = classify(&[16, 116, 17, 117, 18, 118], &rules).expect("three-pair tractor");
    let hand = vec![15, 20, 120, 320, 21, 22, 122, 222, 322, 23];
    let follow = forced_follow(&hand, &lead, &rules).expect("forced follow");

    assert!(follow_is_legal(&hand, &follow, &lead, &rules));
    assert_eq!(count_group_pairs(&follow, lead.suit, &rules), 3);
}

#[test]
fn higher_pair_beats_lower_pair() {
    let rules = rules(TractorRank::TWO);
    let trick = [
        played(0, vec![2, 102]), // suit0 rank3 pair
        played(1, vec![5, 105]), // suit0 rank6 pair beats
    ];
    assert_eq!(trick_winner(&trick, &rules), Some(1));
}

#[test]
fn higher_same_suit_single_wins_but_off_suit_cannot() {
    let rules = rules(TractorRank::TWO);
    let trick = [
        played(0, vec![5]),  // suit0 rank6 leads
        played(1, vec![6]),  // suit0 rank7 beats
        played(2, vec![18]), // suit1: off-suit, cannot win
        played(3, vec![4]),  // suit0 rank5, below the lead
    ];
    assert_eq!(trick_winner(&trick, &rules), Some(1));
}

#[test]
fn identical_cards_form_a_pair_but_same_rank_does_not() {
    let rules = rules(TractorRank::TWO);
    // 2 and 102 are both (suit 0, rank 3): a real pair.
    assert!(matches!(
        classify(&[2, 102], &rules).map(|c| c.kind),
        Some(ComboKind::Pair)
    ));
    // 4 (suit 0, rank 5) and 17 (suit 1, rank 5): same rank, different suits.
    assert!(classify(&[4, 17], &rules).is_none());
}

#[test]
fn must_follow_suit_and_preserve_pairs() {
    let rules = rules(TractorRank::TWO);
    // Lead a suit-0 pair; hand holds a suit-0 pair plus off-suit cards.
    let lead = classify(&[2, 102], &rules).unwrap();
    let hand = vec![3, 103, 20, 21];
    // Two suit-0 singles instead of the available pair: illegal.
    assert!(!follow_is_legal(&hand, &[3, 20], &lead, &rules));
    // Playing the suit-0 pair: legal.
    assert!(follow_is_legal(&hand, &[3, 103], &lead, &rules));
}

fn played(position: i32, cards: Vec<i32>) -> WsTractorPlayedCards {
    WsTractorPlayedCards {
        position,
        name: String::new(),
        cards,
    }
}

// Card encoding (deck copy 0): base 1..13 = suit 0 ranks 2..A, 14..26 = suit 1,
// 27..39 = suit 2, 40..52 = suit 3, 53 = small joker, 54 = big joker. A second
// deck copy adds 100, so 2 and 102 are the identical card (suit 0, rank 3).
fn rules(target: TractorRank) -> TractorRules {
    TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        deck_count: 2,
        final_target_rank: TractorRank::A,
        target_rank: target,
        trump_suit: None,
    }
}

#[test]
fn tractor_needs_consecutive_identity_pairs() {
    let rules = rules(TractorRank::TWO);
    // (suit0 rank3)² + (suit0 rank4)² = a length-2 tractor.
    assert!(matches!(
        classify(&[2, 102, 3, 103], &rules).map(|c| c.kind),
        Some(ComboKind::Tractor(2))
    ));
    // rank3² + rank5² leaves a gap: a two-pair throw, not a tractor.
    assert_eq!(
        classify(&[2, 102, 4, 104], &rules).map(|combo| combo.kind),
        Some(ComboKind::Throw { cards: 4, pairs: 2 })
    );
}

#[test]
fn three_deck_consecutive_triples_form_titanic_only_in_tractor() {
    let mut three_deck = rules(TractorRank::TWO);
    three_deck.deck_count = 3;
    let cards = [2, 102, 202, 3, 103, 203];
    assert_eq!(
        classify(&cards, &three_deck).map(|combo| combo.kind),
        Some(ComboKind::Titanic(2))
    );

    let two_deck = rules(TractorRank::TWO);
    assert_eq!(
        classify(&cards, &two_deck).map(|combo| combo.kind),
        Some(ComboKind::Throw { cards: 6, pairs: 2 })
    );
}

#[test]
fn bottom_multiplier_uses_the_strongest_standard_shape() {
    let two_deck = rules(TractorRank::TWO);
    assert_eq!(bottom_multiplier(&[2], &two_deck), 2);
    assert_eq!(bottom_multiplier(&[2, 102], &two_deck), 4);
    assert_eq!(bottom_multiplier(&[2, 102, 3, 103], &two_deck), 8);
    assert_eq!(bottom_multiplier(&[2, 102, 3, 103, 4, 104], &two_deck), 16);

    let mut three_deck = two_deck;
    three_deck.deck_count = 3;
    assert_eq!(bottom_multiplier(&[2, 102, 202], &three_deck), 6);
    assert_eq!(
        bottom_multiplier(&[2, 102, 202, 3, 103, 203], &three_deck),
        18
    );
    assert_eq!(
        bottom_multiplier(&[2, 102, 202, 12, 112, 13], &three_deck),
        6
    );
}

#[test]
fn higher_titanic_wins_and_followers_must_preserve_triples() {
    let mut rules = rules(TractorRank::TWO);
    rules.deck_count = 3;
    let lead_cards = vec![2, 102, 202, 3, 103, 203];
    let trick = [
        played(0, lead_cards.clone()),
        played(1, vec![3, 103, 203, 4, 104, 204]),
    ];
    assert_eq!(trick_winner(&trick, &rules), Some(1));

    let lead = classify(&lead_cards, &rules).unwrap();
    let hand = vec![4, 104, 204, 5, 6, 7, 8, 9, 10];
    assert!(!follow_is_legal(&hand, &[5, 6, 7, 8, 9, 10], &lead, &rules));
    assert!(follow_is_legal(
        &hand,
        &[4, 104, 204, 5, 6, 7],
        &lead,
        &rules
    ));
    let forced = forced_follow(&hand, &lead, &rules).unwrap();
    assert!(follow_is_legal(&hand, &forced, &lead, &rules));
    assert_eq!(count_group_triples(&forced, lead.suit, &rules), 1);
}

#[test]
fn enumerate_leads_includes_titanic_in_three_deck_game() {
    let mut rules = rules(TractorRank::TWO);
    rules.deck_count = 3;
    let hand = vec![2, 102, 202, 3, 103, 203, 20];
    let leads = enumerate_leads(&hand, &rules);
    assert!(leads.iter().any(|cards| {
        classify(cards, &rules).map(|combo| combo.kind) == Some(ComboKind::Titanic(2))
    }));
}

#[test]
fn failed_three_deck_throw_exposes_titanic_component() {
    let mut rules = rules(TractorRank::TWO);
    rules.deck_count = 3;
    let cards = vec![2, 102, 202, 3, 103, 203, 4];
    let components = throw_components(&cards, &rules).unwrap();
    assert!(components.iter().any(|component| {
        component.len() == 6
            && classify(component, &rules).map(|combo| combo.kind) == Some(ComboKind::Titanic(2))
    }));
}

#[test]
fn trump_beats_any_plain_lead() {
    let rules = rules(TractorRank::TWO);
    let trick = [
        played(0, vec![6]), // suit0 rank7 leads
        played(1, vec![1]), // suit0 rank2 = trump, ruffs in
    ];
    assert_eq!(trick_winner(&trick, &rules), Some(1));
}

#[test]
fn trump_rank_closes_the_tractor_gap() {
    // Target rank 5 is trump, so suit-0 rank4 and rank6 become adjacent.
    let rules = rules(TractorRank::FIVE);
    // rank4 = base 3, rank6 = base 5.
    assert!(matches!(
        classify(&[3, 103, 5, 105], &rules).map(|c| c.kind),
        Some(ComboKind::Tractor(2))
    ));
}

#[test]
fn ace_level_keeps_trump_king_and_main_suit_two_as_a_tractor() {
    let mut rules = rules(TractorRank::A);
    rules.trump_suit = Some(share_type_public::TractorSuit::HEART);

    assert_eq!(
        classify(&[25, 125, 14, 114], &rules).map(|combo| combo.kind),
        Some(ComboKind::Tractor(2)),
    );
}

#[test]
fn trump_tractor_edges_keep_level_and_joker_order() {
    let mut rules = rules(TractorRank::THREE);
    rules.trump_suit = Some(share_type_public::TractorSuit::HEART);

    for cards in [
        [26, 126, 14, 114], // 主 A -> 主 2
        [14, 114, 2, 102],  // 主 2 -> 副级
        [2, 102, 15, 115],  // 副级 -> 主级
        [15, 115, 53, 153], // 主级 -> 小王
        [53, 153, 54, 154], // 小王 -> 大王
        [25, 125, 26, 126], // 普通主牌保持连续
    ] {
        assert_eq!(
            classify(&cards, &rules).map(|combo| combo.kind),
            Some(ComboKind::Tractor(2)),
            "expected adjacent trump pairs for {cards:?}",
        );
    }

    for cards in [
        [26, 126, 2, 102],  // 跳过主 2
        [14, 114, 15, 115], // 跳过副级
        [2, 102, 53, 153],  // 跳过主级
        [15, 115, 54, 154], // 跳过小王
    ] {
        assert_eq!(
            classify(&cards, &rules).map(|combo| combo.kind),
            Some(ComboKind::Throw { cards: 4, pairs: 2 }),
            "skipping a trump layer must not form a tractor for {cards:?}",
        );
    }
}

#[test]
fn trump_titanic_edges_use_the_same_strict_special_sequence() {
    let mut rules = rules(TractorRank::THREE);
    rules.deck_count = 3;
    rules.trump_suit = Some(share_type_public::TractorSuit::HEART);

    for cards in [
        [26, 126, 226, 14, 114, 214],
        [14, 114, 214, 2, 102, 202],
        [2, 102, 202, 15, 115, 215],
        [15, 115, 215, 53, 153, 253],
        [53, 153, 253, 54, 154, 254],
    ] {
        assert_eq!(
            classify(&cards, &rules).map(|combo| combo.kind),
            Some(ComboKind::Titanic(2)),
            "expected adjacent permanent-trump triples for {cards:?}",
        );
    }

    for cards in [
        [26, 126, 226, 2, 102, 202],
        [14, 114, 214, 15, 115, 215],
        [2, 102, 202, 53, 153, 253],
        [15, 115, 215, 54, 154, 254],
    ] {
        assert_eq!(
            classify(&cards, &rules).map(|combo| combo.kind),
            Some(ComboKind::Throw { cards: 6, pairs: 2 }),
            "skipping a permanent-trump layer must not form a Titanic for {cards:?}",
        );
    }
}

#[test]
fn ordinary_off_suit_two_lead_follows_its_natural_suit() {
    let mut rules = rules(TractorRank::THREE);
    rules.trump_suit = Some(share_type_public::TractorSuit::HEART);
    let lead = classify(&[1, 101], &rules).expect("ordinary off-suit two pair lead");
    let hand = vec![27, 127, 3, 103];

    assert!(follow_is_legal(&hand, &[3, 103], &lead, &rules));
    assert!(!follow_is_legal(&hand, &[27, 127], &lead, &rules));
}

#[test]
fn follow_uses_every_available_lead_group_card_for_each_lead_size() {
    let rules = rules(TractorRank::A);

    for lead_len in 1..=8 {
        let lead_cards = (0..lead_len)
            .map(|offset| 2 + offset as i32)
            .collect::<Vec<_>>();
        let lead = classify(&lead_cards, &rules).expect("same-suit lead");
        for held_group_count in 0..=lead_len + 2 {
            let group_cards = (0..lead_len + 2)
                .map(|offset| 102 + offset as i32)
                .collect::<Vec<_>>();
            let outside_cards = (0..lead_len + 1)
                .map(|offset| 15 + offset as i32)
                .collect::<Vec<_>>();
            let mut hand = group_cards[..held_group_count].to_vec();
            hand.extend_from_slice(&outside_cards);

            let required_group_count = held_group_count.min(lead_len);
            let forced = forced_follow(&hand, &lead, &rules)
                .expect("a hand with enough total cards always has a forced follow");
            assert_eq!(forced.len(), lead_len);
            assert_eq!(
                forced
                    .iter()
                    .filter(|card| card_in_group(**card, lead.suit, &rules))
                    .count(),
                required_group_count,
                "lead_len={lead_len}, held_group_count={held_group_count}, forced={forced:?}",
            );
            assert!(follow_is_legal(&hand, &forced, &lead, &rules));

            if required_group_count == 0 {
                continue;
            }
            let mut under_follow = group_cards[..required_group_count - 1].to_vec();
            under_follow.extend_from_slice(
                &outside_cards[..lead_len.saturating_sub(required_group_count - 1)],
            );
            assert_eq!(under_follow.len(), lead_len);
            assert!(
                !follow_is_legal(&hand, &under_follow, &lead, &rules),
                "must not omit one of {required_group_count} available group cards for a {lead_len}-card lead",
            );
        }
    }
}

#[test]
fn void_in_led_suit_allows_any_cards() {
    let rules = rules(TractorRank::TWO);
    let lead = classify(&[2, 102], &rules).unwrap();
    // Hand has no suit-0 cards, so any two cards follow.
    let hand = vec![20, 21, 34];
    assert!(follow_is_legal(&hand, &[20, 34], &lead, &rules));
}
