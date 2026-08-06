use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use rand::{SeedableRng, rngs::StdRng, seq::IndexedRandom};
use share_type_public::{UpgradePhase, UpgradeRank};
use upgrade::{
    UpgradeDeckCount,
    combo::{self, ComboKind, UpgradeComboRules},
    state::{UpgradeGameState, UpgradeRules, build_upgrade_deck_with_removed_ranks},
};
use upgrade_common::{Card, Rank, Suit};
use ws_common::CommonGameState;

const SINGLE: u8 = 1;
const PAIR: u8 = 2;
const TRIPLE: u8 = 4;
const THROW: u8 = 8;
const ALL_KINDS: u8 = SINGLE | PAIR | TRIPLE | THROW;

fn combo_kind_bit(kind: ComboKind) -> u8 {
    match kind {
        ComboKind::Single => SINGLE,
        ComboKind::Pair => PAIR,
        ComboKind::Triple => TRIPLE,
        ComboKind::Throw { .. } => THROW,
    }
}

fn decode_cards(cards: &[i32]) -> Vec<Card> {
    cards
        .iter()
        .copied()
        .map(Card::try_from)
        .collect::<Result<_, _>>()
        .expect("random upgrade cards must decode")
}

fn push_unique(candidates: &mut Vec<Vec<i32>>, cards: Vec<i32>, rules: UpgradeComboRules) {
    if combo::classify(&decode_cards(&cards), rules).is_some() && !candidates.contains(&cards) {
        candidates.push(cards);
    }
}

fn lead_candidates(hand: &[i32], rules: UpgradeComboRules) -> Vec<Vec<i32>> {
    let mut candidates = hand.iter().map(|card| vec![*card]).collect::<Vec<_>>();
    let mut groups: HashMap<Option<Suit>, HashMap<u8, Vec<i32>>> = HashMap::new();
    for encoded in hand {
        let card = Card::try_from(*encoded).expect("random upgrade hand card");
        groups
            .entry(combo::card_group(card, rules))
            .or_default()
            .entry(card.identity())
            .or_default()
            .push(*encoded);
    }

    for identities in groups.values_mut() {
        let mut components = identities.values_mut().collect::<Vec<_>>();
        for component in &mut components {
            component.sort_unstable();
        }
        components.sort_by_key(|component| {
            component
                .first()
                .and_then(|encoded| Card::try_from(*encoded).ok())
                .map(Card::identity)
                .unwrap_or_default()
        });

        for component in &components {
            for count in 2..=component.len() {
                push_unique(&mut candidates, component[..count].to_vec(), rules);
            }
        }
        for window_size in 2..=components.len().min(3) {
            for window in components.windows(window_size) {
                push_unique(
                    &mut candidates,
                    window
                        .iter()
                        .flat_map(|component| component.iter().copied())
                        .collect(),
                    rules,
                );
            }
        }
        if components.len() >= 2 {
            push_unique(
                &mut candidates,
                components
                    .iter()
                    .flat_map(|component| component.iter().copied())
                    .collect(),
                rules,
            );
        }
    }
    candidates
}

fn assert_card_conservation(state: &UpgradeGameState, played_cards: &[i32], expected: usize) {
    let cards = state
        .hands
        .values()
        .flatten()
        .copied()
        .chain(state.bottom_cards.iter().copied())
        .chain(played_cards.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), expected);
    assert!(cards.iter().all(|card| Card::try_from(*card).is_ok()));
    assert_eq!(
        cards.iter().copied().collect::<HashSet<_>>().len(),
        expected
    );
}

fn simulate_random_round(deck_count: u8, seed: u64) -> u8 {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    for position in 0..4 {
        common.lock().unwrap().add_player(
            position,
            position as u64 + 1,
            &format!("random-{deck_count}-{seed}-{position}"),
        );
    }
    let deck_count = UpgradeDeckCount::new(deck_count).expect("random upgrade deck count");
    let removed_rank_count = seed as usize % 10;
    let rules = UpgradeRules {
        deck_count,
        target_rank: Rank::Three,
        final_target_rank: Rank::Ace,
        removed_rank_count,
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        trump_suit: None,
    };
    let expected_card_count =
        build_upgrade_deck_with_removed_ranks(deck_count, removed_rank_count).len();
    let mut state = UpgradeGameState::from_common(common);
    state
        .deal_new_round(rules)
        .expect("random upgrade deal should start");
    while state.phase == UpgradePhase::Deal {
        state
            .deal_next_card()
            .expect("random upgrade incremental deal");
    }
    assert_eq!(state.phase, UpgradePhase::Bury);
    assert!(state.declaration.is_some());
    assert!(state.rules.trump_suit.is_some());
    let dealer_position = state.dealer_position;
    let bottom_cards = state.bottom_cards.clone();
    state
        .bury_bottom(dealer_position, bottom_cards)
        .expect("random upgrade bury");
    assert_eq!(state.phase, UpgradePhase::Play);
    assert_card_conservation(&state, &[], expected_card_count);

    let combo_rules = UpgradeComboRules {
        target_rank: state.rules.target_rank,
        trump_suit: state.rules.trump_suit,
    };
    let mut rng = StdRng::seed_from_u64(seed ^ (u64::from(deck_count.get()) << 32));
    let mut played_cards = Vec::with_capacity(expected_card_count - state.bottom_cards.len());
    let mut lead_kinds = 0_u8;
    let mut action_count = 0_usize;
    while state.phase == UpgradePhase::Play {
        action_count += 1;
        assert!(
            action_count <= expected_card_count,
            "random upgrade round did not converge for {deck_count:?} decks seed {seed}"
        );
        let position = state.current_position;
        let hand = state.private_hand(position);
        let attempted = if state.current_trick.is_empty() {
            let candidates = lead_candidates(&hand, combo_rules);
            let missing = candidates
                .iter()
                .filter(|cards| {
                    combo::classify(&decode_cards(cards), combo_rules)
                        .is_some_and(|combo| combo_kind_bit(combo.kind) & !lead_kinds != 0)
                })
                .collect::<Vec<_>>();
            let selected = if missing.is_empty() {
                candidates.choose(&mut rng).expect("random upgrade lead")
            } else {
                missing.choose(&mut rng).expect("missing upgrade lead kind")
            };
            let kind = combo::classify(&decode_cards(selected), combo_rules)
                .expect("random upgrade lead shape")
                .kind;
            lead_kinds |= combo_kind_bit(kind);
            selected.clone()
        } else {
            let lead = combo::classify(&decode_cards(&state.current_trick[0].cards), combo_rules)
                .expect("random upgrade established lead");
            combo::forced_follow(&decode_cards(&hand), &lead, combo_rules)
                .expect("random upgrade legal follow")
                .into_iter()
                .map(Card::encoded)
                .collect()
        };

        let remaining_before = state.hands.values().map(Vec::len).sum::<usize>();
        let played = state
            .play_cards(position, attempted)
            .unwrap_or_else(|error| panic!("seed {seed} rejected generated play: {error}"));
        assert!(!played.played_cards.is_empty());
        let remaining_after = state.hands.values().map(Vec::len).sum::<usize>();
        assert_eq!(
            remaining_before - remaining_after,
            played.played_cards.len()
        );
        played_cards.extend_from_slice(&played.played_cards);
        assert_card_conservation(&state, &played_cards, expected_card_count);
        if state.current_trick.is_empty() && state.phase == UpgradePhase::Play {
            assert_eq!(
                state
                    .hands
                    .values()
                    .map(Vec::len)
                    .collect::<HashSet<_>>()
                    .len(),
                1,
                "completed trick must restore equal upgrade hands"
            );
        }
    }

    assert_eq!(state.phase, UpgradePhase::Settlement);
    assert!(state.hands.values().all(Vec::is_empty));
    assert_eq!(
        played_cards.len() + state.bottom_cards.len(),
        expected_card_count
    );
    assert_card_conservation(&state, &played_cards, expected_card_count);
    let settlement = state.settlement_event();
    assert_eq!(settlement.player_scores.len(), 4);
    assert_eq!(settlement.target_rank, state.target_rank_protocol());
    assert!(matches!(
        settlement.next_target_rank,
        None | Some(UpgradeRank::FOUR)
            | Some(UpgradeRank::FIVE)
            | Some(UpgradeRank::SIX)
            | Some(UpgradeRank::SEVEN)
            | Some(UpgradeRank::EIGHT)
            | Some(UpgradeRank::NINE)
            | Some(UpgradeRank::TEN)
            | Some(UpgradeRank::J)
            | Some(UpgradeRank::Q)
            | Some(UpgradeRank::K)
            | Some(UpgradeRank::A)
    ));
    lead_kinds
}

macro_rules! randomized_cases {
    ($deck_count:expr, $(($name:ident, $start:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let mut covered = 0;
                for seed in $start..($start + 8) {
                    covered |= simulate_random_round($deck_count, seed);
                }
                assert_eq!(covered, ALL_KINDS);
            }
        )+
    };
}

randomized_cases!(
    3,
    (three_deck_seeds_000_007, 0),
    (three_deck_seeds_008_015, 8),
    (three_deck_seeds_016_023, 16),
    (three_deck_seeds_024_031, 24),
    (three_deck_seeds_032_039, 32),
    (three_deck_seeds_040_047, 40),
    (three_deck_seeds_048_055, 48),
    (three_deck_seeds_056_063, 56),
    (three_deck_seeds_064_071, 64),
    (three_deck_seeds_072_079, 72),
    (three_deck_seeds_080_087, 80),
    (three_deck_seeds_088_095, 88),
    (three_deck_seeds_096_103, 96),
    (three_deck_seeds_104_111, 104),
    (three_deck_seeds_112_119, 112),
    (three_deck_seeds_120_127, 120),
);

randomized_cases!(
    4,
    (four_deck_seeds_128_135, 128),
    (four_deck_seeds_136_143, 136),
    (four_deck_seeds_144_151, 144),
    (four_deck_seeds_152_159, 152),
);

randomized_cases!(
    5,
    (five_deck_seeds_160_167, 160),
    (five_deck_seeds_168_175, 168),
    (five_deck_seeds_176_183, 176),
    (five_deck_seeds_184_191, 184),
);

randomized_cases!(
    6,
    (six_deck_seeds_192_199, 192),
    (six_deck_seeds_200_207, 200),
    (six_deck_seeds_208_215, 208),
    (six_deck_seeds_216_223, 216),
    (six_deck_seeds_224_231, 224),
    (six_deck_seeds_232_239, 232),
    (six_deck_seeds_240_247, 240),
    (six_deck_seeds_248_255, 248),
);
