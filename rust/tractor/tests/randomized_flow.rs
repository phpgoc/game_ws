use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use rand::{SeedableRng, rngs::StdRng, seq::IndexedRandom};
use share_type_public::{TractorPhase, TractorRank};
use tractor::{
    combo,
    game_state::{TractorGameState, TractorRules},
};
use upgrade_common::Card;
use ws_common::CommonGameState;

fn card_count(state: &TractorGameState) -> usize {
    state.hands.values().map(Vec::len).sum::<usize>()
        + state.bottom_cards.len()
        + state
            .current_trick
            .iter()
            .map(|played| played.cards.len())
            .sum::<usize>()
        + state
            .completed_tricks
            .iter()
            .flatten()
            .map(|played| played.cards.len())
            .sum::<usize>()
}

fn physical_cards(state: &TractorGameState) -> Vec<i32> {
    state
        .hands
        .values()
        .flatten()
        .copied()
        .chain(state.bottom_cards.iter().copied())
        .chain(
            state
                .current_trick
                .iter()
                .flat_map(|played| played.cards.iter().copied()),
        )
        .chain(
            state
                .completed_tricks
                .iter()
                .flatten()
                .flat_map(|played| played.cards.iter().copied()),
        )
        .collect()
}

fn assert_card_conservation(state: &TractorGameState, expected: usize) {
    assert_eq!(card_count(state), expected);
    let cards = physical_cards(state);
    assert_eq!(cards.len(), expected);
    assert!(cards.iter().all(|card| Card::try_from(*card).is_ok()));
    assert_eq!(
        cards.iter().copied().collect::<HashSet<_>>().len(),
        expected
    );
}

fn simulate_random_round(deck_count: usize, seed: u64) {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    for position in 0..4 {
        common.lock().unwrap().add_player(
            position,
            position as u64 + 1,
            &format!("random-{seed}-{position}"),
        );
    }
    let rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: if deck_count == 3 { 10 } else { 8 },
        deck_count,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::THREE,
        trump_suit: None,
    };
    let mut state = TractorGameState::from_common(common);
    state.deal_new_round(rules).expect("random tractor deal");
    while state.phase == TractorPhase::Deal {
        state
            .deal_next_card()
            .expect("random tractor incremental deal");
    }
    assert_eq!(state.phase, TractorPhase::Bury);
    assert!(state.declaration.is_some());
    let dealer_position = state.dealer_position;
    let bottom_cards = state.bottom_cards.clone();
    state
        .bury_bottom(dealer_position, bottom_cards)
        .expect("random tractor bury");
    assert_eq!(state.phase, TractorPhase::Play);

    let expected_card_count = deck_count * 54;
    assert_card_conservation(&state, expected_card_count);
    let mut rng = StdRng::seed_from_u64(seed);
    let mut action_count = 0_usize;
    while state.phase == TractorPhase::Play {
        action_count += 1;
        assert!(
            action_count <= expected_card_count,
            "random tractor round did not converge for seed {seed}"
        );
        let position = state.current_position;
        let hand = state
            .hands
            .get(&position)
            .cloned()
            .expect("random tractor current hand");
        let attempted = if state.current_trick.is_empty() {
            let leads = combo::enumerate_leads(&hand, &state.rules);
            leads
                .choose(&mut rng)
                .cloned()
                .expect("random tractor legal lead")
        } else {
            let lead = combo::classify(&state.current_trick[0].cards, &state.rules)
                .expect("random tractor established lead");
            let follows = combo::enumerate_follows(&hand, &lead, &state.rules);
            follows
                .choose(&mut rng)
                .cloned()
                .expect("random tractor legal follow")
        };
        let remaining_before = state.hands.values().map(Vec::len).sum::<usize>();
        let played = state
            .play_cards(position, format!("p{position}"), attempted)
            .unwrap_or_else(|error| panic!("seed {seed} rejected enumerated play: {error}"));
        assert!(!played.cards.is_empty());
        let remaining_after = state.hands.values().map(Vec::len).sum::<usize>();
        assert_eq!(remaining_before - remaining_after, played.cards.len());
        assert_card_conservation(&state, expected_card_count);
        if state.current_trick.is_empty() && state.phase == TractorPhase::Play {
            let hand_sizes = state.hands.values().map(Vec::len).collect::<HashSet<_>>();
            assert_eq!(
                hand_sizes.len(),
                1,
                "completed trick must restore equal hands"
            );
        }
    }

    assert_eq!(state.phase, TractorPhase::Settlement);
    assert!(state.hands.values().all(Vec::is_empty));
    assert_card_conservation(&state, expected_card_count);
    let settlement = state.next_target_rank();
    assert!(settlement.is_some());
    assert_eq!(state.player_scores_snapshot().len(), 4);
}

macro_rules! randomized_cases {
    ($deck_count:expr, $(($name:ident, $start:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                for seed in $start..($start + 8) {
                    simulate_random_round($deck_count, seed);
                }
            }
        )+
    };
}

randomized_cases!(
    2,
    (two_deck_seeds_000_007, 0),
    (two_deck_seeds_008_015, 8),
    (two_deck_seeds_016_023, 16),
    (two_deck_seeds_024_031, 24),
    (two_deck_seeds_032_039, 32),
    (two_deck_seeds_040_047, 40),
    (two_deck_seeds_048_055, 48),
    (two_deck_seeds_056_063, 56),
    (two_deck_seeds_064_071, 64),
    (two_deck_seeds_072_079, 72),
    (two_deck_seeds_080_087, 80),
    (two_deck_seeds_088_095, 88),
    (two_deck_seeds_096_103, 96),
    (two_deck_seeds_104_111, 104),
    (two_deck_seeds_112_119, 112),
    (two_deck_seeds_120_127, 120),
);

randomized_cases!(
    3,
    (three_deck_seeds_128_135, 128),
    (three_deck_seeds_136_143, 136),
    (three_deck_seeds_144_151, 144),
    (three_deck_seeds_152_159, 152),
    (three_deck_seeds_160_167, 160),
    (three_deck_seeds_168_175, 168),
    (three_deck_seeds_176_183, 176),
    (three_deck_seeds_184_191, 184),
    (three_deck_seeds_192_199, 192),
    (three_deck_seeds_200_207, 200),
    (three_deck_seeds_208_215, 208),
    (three_deck_seeds_216_223, 216),
    (three_deck_seeds_224_231, 224),
    (three_deck_seeds_232_239, 232),
    (three_deck_seeds_240_247, 240),
    (three_deck_seeds_248_255, 248),
);
