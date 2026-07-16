use std::sync::{Arc, Mutex};

use share_type_public::LandlordPhase;
use ws_common::CommonGameState;

use crate::{
    ai::{
        AiObservation, CardBelief, Relationship, candidates::all_candidates, choose_bid,
        choose_play, playing::choose_heuristic_play,
    },
    core::play::{ComboKind, can_beat, classify},
    game_state::{LandlordLoopState, LandlordPlayRecord},
    play_validator::validate_play_request,
};

const POSITIONS: [usize; 3] = [0, 1, 2];

#[test]
fn deterministic_ai_rounds_finish_with_legal_public_information_decisions() {
    let mut completed_rounds = 0;
    let mut landlord_wins = 0;
    let mut farmer_wins = 0;

    for seed in 1..=128 {
        let mut state = dealt_state(seed);
        let Some(landlord) = run_bidding(&mut state) else {
            continue;
        };
        let (winner, actions) = play_to_completion(&mut state, seed);

        assert!(actions <= 180, "round {seed} made too little progress");
        if winner == landlord {
            landlord_wins += 1;
        } else {
            farmer_wins += 1;
        }
        completed_rounds += 1;
        if completed_rounds == 24 {
            break;
        }
    }

    assert_eq!(
        completed_rounds, 24,
        "too many deals ended with all players passing"
    );
    assert!(
        landlord_wins > 0,
        "simulation never produced a landlord win"
    );
    assert!(farmer_wins > 0, "simulation never produced a farmer win");
}

#[test]
fn evolved_ai_beats_a_fixed_greedy_baseline_with_rotated_roles() {
    let mut evolved_wins = 0;
    let mut matches = 0;
    for seed in 1..=12 {
        let landlord = (seed as usize - 1) % POSITIONS.len();

        let mut evolved_landlord = prepared_play_state(seed, landlord);
        let winner = play_mixed_match(&mut evolved_landlord, landlord, true, seed);
        evolved_wins += usize::from(winner == landlord);
        matches += 1;

        let mut evolved_farmers = prepared_play_state(seed, landlord);
        let winner = play_mixed_match(&mut evolved_farmers, landlord, false, seed);
        evolved_wins += usize::from(winner != landlord);
        matches += 1;
    }

    assert!(
        evolved_wins * 4 >= matches * 3,
        "evolved AI won only {evolved_wins}/{matches} fixed matches against the greedy baseline"
    );
}

#[test]
fn search_ai_beats_the_same_team_heuristic_without_search() {
    let mut search_wins = 0;
    let mut matches = 0;
    for seed in 101..=112 {
        let landlord = (seed as usize - 1) % POSITIONS.len();

        let mut search_landlord = prepared_play_state(seed, landlord);
        let winner = play_policy_match(&mut search_landlord, landlord, true, seed);
        search_wins += usize::from(winner == landlord);
        matches += 1;

        let mut search_farmers = prepared_play_state(seed, landlord);
        let winner = play_policy_match(&mut search_farmers, landlord, false, seed);
        search_wins += usize::from(winner != landlord);
        matches += 1;
    }

    assert!(
        search_wins * 2 > matches,
        "belief search won only {search_wins}/{matches} matches against its strong heuristic ablation"
    );
}

fn dealt_state(seed: u64) -> LandlordLoopState {
    let mut common = CommonGameState::new();
    for position in POSITIONS {
        common.add_player(position, position as u64 + 1, &format!("AI {position}"));
        common.mark_ai_position(position);
    }
    let mut state = LandlordLoopState::new(Arc::new(Mutex::new(common)));
    let deck = shuffled_deck(seed);
    for (index, position) in POSITIONS.into_iter().enumerate() {
        let mut hand = deck[index * 17..(index + 1) * 17].to_vec();
        hand.sort_unstable();
        state.hands.insert(position, hand);
    }
    state.hidden_cards = deck[51..].to_vec();
    state.phase = LandlordPhase::CallLandlord;
    state
}

fn prepared_play_state(seed: u64, landlord: usize) -> LandlordLoopState {
    let mut state = dealt_state(seed);
    state.landlord_position = Some(landlord);
    state.phase = LandlordPhase::Play;
    state.current_position = landlord;
    state.last_play_position = landlord;
    state.score = 1;
    state
        .hands
        .get_mut(&landlord)
        .expect("landlord hand")
        .extend(&state.hidden_cards);
    state
        .hands
        .get_mut(&landlord)
        .expect("landlord hand")
        .sort_unstable();
    state
}

fn run_bidding(state: &mut LandlordLoopState) -> Option<usize> {
    for position in POSITIONS {
        state.current_position = position;
        let previous_score = state.score as u8;
        let score = choose_bid(state, position);
        assert!(score == 0 || score > previous_score);
        assert!(score <= 3);
        state.call_history.push((position, score));
        if score > 0 {
            state.score = u32::from(score);
        }
        if score == 3 {
            break;
        }
    }

    let maximum = state.call_history.iter().map(|(_, score)| *score).max()?;
    if maximum == 0 {
        return None;
    }
    let landlord = state
        .call_history
        .iter()
        .rev()
        .find_map(|(position, score)| (*score == maximum).then_some(*position))?;
    state.landlord_position = Some(landlord);
    state.phase = LandlordPhase::Play;
    state.current_position = landlord;
    state.last_play_position = landlord;
    state.hands.get_mut(&landlord)?.extend(&state.hidden_cards);
    state.hands.get_mut(&landlord)?.sort_unstable();
    Some(landlord)
}

fn play_to_completion(state: &mut LandlordLoopState, seed: u64) -> (usize, usize) {
    for action in 1..=180 {
        let position = state.current_position;
        assert_public_card_knowledge_is_consistent(state, position);

        let cards = choose_play(state, position);
        assert!(
            validate_play_request(state, position, &cards),
            "illegal AI play in round {seed}, action {action}, position {position}: {cards:?}"
        );

        let benchmark = if state.last_play.is_empty() || state.last_play_position == position {
            Vec::new()
        } else {
            state.last_play.clone()
        };
        state.play_history.push(LandlordPlayRecord {
            position,
            cards: cards.clone(),
            benchmark,
        });

        if !cards.is_empty() {
            state.last_play_position = position;
            state.last_play = cards.clone();
            let hand = state.hands.get_mut(&position).expect("current hand");
            for card in cards {
                let index = hand
                    .iter()
                    .position(|candidate| *candidate == card)
                    .expect("AI only plays cards in its hand");
                hand.remove(index);
            }
            if hand.is_empty() {
                return (position, action);
            }
        }

        let index = POSITIONS
            .iter()
            .position(|candidate| *candidate == position)
            .expect("current position");
        state.current_position = POSITIONS[(index + 1) % POSITIONS.len()];
        if state.last_play_position == state.current_position {
            state.last_play.clear();
        }
    }
    panic!("AI round {seed} did not finish within the action limit");
}

fn play_mixed_match(
    state: &mut LandlordLoopState,
    landlord: usize,
    evolved_controls_landlord: bool,
    seed: u64,
) -> usize {
    for action in 1..=180 {
        let position = state.current_position;
        let uses_evolved = (position == landlord) == evolved_controls_landlord;
        let cards = if uses_evolved {
            choose_play(state, position)
        } else {
            greedy_baseline_play(state, position)
        };
        assert!(
            validate_play_request(state, position, &cards),
            "illegal mixed-policy play in round {seed}, action {action}, position {position}: {cards:?}"
        );
        let benchmark = if state.last_play.is_empty() || state.last_play_position == position {
            Vec::new()
        } else {
            state.last_play.clone()
        };
        state.play_history.push(LandlordPlayRecord {
            position,
            cards: cards.clone(),
            benchmark,
        });
        if !cards.is_empty() {
            state.last_play_position = position;
            state.last_play = cards.clone();
            let hand = state.hands.get_mut(&position).expect("current hand");
            for card in cards {
                let index = hand
                    .iter()
                    .position(|candidate| *candidate == card)
                    .expect("policy only plays held cards");
                hand.remove(index);
            }
            if hand.is_empty() {
                return position;
            }
        }
        let index = POSITIONS
            .iter()
            .position(|candidate| *candidate == position)
            .expect("current position");
        state.current_position = POSITIONS[(index + 1) % POSITIONS.len()];
        if state.last_play_position == state.current_position {
            state.last_play.clear();
        }
    }
    panic!("mixed-policy round {seed} did not finish within the action limit");
}

fn play_policy_match(
    state: &mut LandlordLoopState,
    landlord: usize,
    search_controls_landlord: bool,
    seed: u64,
) -> usize {
    for action in 1..=180 {
        let position = state.current_position;
        let uses_search = (position == landlord) == search_controls_landlord;
        let cards = if uses_search {
            choose_play(state, position)
        } else {
            let observation = AiObservation::from_state(state, position).expect("observation");
            choose_heuristic_play(&observation)
        };
        assert!(
            validate_play_request(state, position, &cards),
            "illegal policy-ablation play in round {seed}, action {action}, position {position}: {cards:?}"
        );
        apply_simulated_play(state, position, cards);
        if state.hands[&position].is_empty() {
            return position;
        }
        advance_simulated_turn(state, position);
    }
    panic!("policy-ablation round {seed} did not finish within the action limit");
}

fn apply_simulated_play(state: &mut LandlordLoopState, position: usize, cards: Vec<i32>) {
    let benchmark = if state.last_play.is_empty() || state.last_play_position == position {
        Vec::new()
    } else {
        state.last_play.clone()
    };
    state.play_history.push(LandlordPlayRecord {
        position,
        cards: cards.clone(),
        benchmark,
    });
    if cards.is_empty() {
        return;
    }
    state.last_play_position = position;
    state.last_play = cards.clone();
    let hand = state.hands.get_mut(&position).expect("current hand");
    for card in cards {
        let index = hand
            .iter()
            .position(|candidate| *candidate == card)
            .expect("policy only plays held cards");
        hand.remove(index);
    }
}

fn advance_simulated_turn(state: &mut LandlordLoopState, position: usize) {
    let index = POSITIONS
        .iter()
        .position(|candidate| *candidate == position)
        .expect("current position");
    state.current_position = POSITIONS[(index + 1) % POSITIONS.len()];
    if state.last_play_position == state.current_position {
        state.last_play.clear();
    }
}

fn greedy_baseline_play(state: &LandlordLoopState, position: usize) -> Vec<i32> {
    let hand = &state.hands[&position];
    let leading = state.last_play.is_empty() || state.last_play_position == position;
    let benchmark = (!leading).then(|| classify(&state.last_play)).flatten();
    let candidates = all_candidates(hand)
        .into_iter()
        .filter(|candidate| {
            benchmark
                .as_ref()
                .is_none_or(|previous| can_beat(&candidate.combo, previous))
        })
        .collect::<Vec<_>>();
    if let Some(finisher) = candidates
        .iter()
        .find(|candidate| candidate.cards.len() == hand.len())
    {
        return finisher.cards.clone();
    }
    candidates
        .iter()
        .filter(|candidate| !matches!(candidate.combo.kind, ComboKind::Bomb | ComboKind::Rocket))
        .min_by_key(|candidate| {
            if leading {
                (
                    usize::MAX - candidate.cards.len(),
                    candidate.combo.main_rank,
                )
            } else {
                (0, candidate.combo.main_rank)
            }
        })
        .or_else(|| candidates.first())
        .map(|candidate| candidate.cards.clone())
        .unwrap_or_default()
}

fn assert_public_card_knowledge_is_consistent(state: &LandlordLoopState, position: usize) {
    let observation = AiObservation::from_state(state, position).expect("AI observation");
    let belief = CardBelief::from_observation(&observation);
    let public_played_count = state
        .play_history
        .iter()
        .map(|record| record.cards.len())
        .sum::<usize>();
    assert_eq!(
        belief
            .played_rank_counts
            .iter()
            .map(|count| usize::from(*count))
            .sum::<usize>(),
        public_played_count
    );

    let actual_outside_count = state
        .hands
        .iter()
        .filter(|(candidate, _)| **candidate != position)
        .map(|(_, cards)| cards.len())
        .sum::<usize>();
    assert_eq!(
        belief
            .remaining_outside_hand
            .iter()
            .map(|count| usize::from(*count))
            .sum::<usize>(),
        actual_outside_count
    );

    for (&opponent, estimate) in &belief.opponents {
        assert_eq!(estimate.hand_size, state.hands[&opponent].len());
        assert_eq!(estimate.relationship, observation.relationship_to(opponent));
        let expected_cards = estimate.expected_rank_counts.iter().sum::<f64>();
        assert!(
            (expected_cards - estimate.hand_size as f64).abs() < 1e-8,
            "expected rank counts do not add up for position {opponent}: {expected_cards}"
        );
        for rank in 3..=17 {
            for count in 0..=4 {
                let probability = estimate.probability_has_at_least(rank, count);
                assert!(
                    (0.0..=1.0).contains(&probability),
                    "invalid probability for observer {position}, opponent {opponent}, rank {rank}, count {count}: {probability}"
                );
            }
        }
    }

    let landlord = observation.landlord_position.expect("landlord");
    for opponent in observation.positions.iter().copied() {
        let expected = if opponent == position {
            Relationship::SelfPlayer
        } else if position != landlord && opponent != landlord {
            Relationship::Ally
        } else {
            Relationship::Enemy
        };
        assert_eq!(observation.relationship_to(opponent), expected);
    }
}

fn shuffled_deck(seed: u64) -> Vec<i32> {
    let mut deck = (1..=54).collect::<Vec<_>>();
    let mut random = seed;
    for index in (1..deck.len()).rev() {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        deck.swap(index, (random >> 32) as usize % (index + 1));
    }
    deck
}
