use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use share_type_public::{DominoesNoPlayableTiles, DominoesPhase, DominoesRule};
use ws_common::CommonGameState;

use super::{DominoesRoundState, Tile};

fn state_with_players(player_count: usize) -> DominoesRoundState {
    let mut common = CommonGameState::new();
    for position in 0..player_count {
        common.add_player(position, position as u64 + 1, &format!("P{position}"));
    }
    DominoesRoundState::new(
        Arc::new(Mutex::new(common)),
        DominoesRule::Simple,
        DominoesNoPlayableTiles::PassWithoutDraw,
        35,
    )
    .expect("four-player state")
}

fn state() -> DominoesRoundState {
    state_with_players(4)
}

#[test]
fn double_six_set_contains_exactly_28_stable_tiles() {
    let tiles = Tile::all();
    assert_eq!(tiles.len(), 28);
    assert_eq!(tiles.first().copied(), Some(Tile { id: 0, a: 0, b: 0 }));
    assert_eq!(tiles.last().copied(), Some(Tile { id: 27, a: 6, b: 6 }));
    assert_eq!(
        tiles.iter().map(|tile| tile.id).collect::<Vec<_>>(),
        (0..28).collect::<Vec<_>>()
    );
}

#[test]
fn three_and_four_players_each_receive_five_tiles() {
    for (player_count, expected_boneyard) in [(3, 13), (4, 8)] {
        let mut state = state_with_players(player_count);
        assert_eq!(state.hand_size(), 5);
        state.start_new_game().expect("start round");
        assert!(state.hands.values().all(|hand| hand.len() == 5));
        assert_eq!(state.boneyard.len(), expected_boneyard);
    }
}

#[test]
fn five_up_scores_open_ends_during_play_and_divides_round_pips() {
    let mut state = state();
    state.rule = DominoesRule::FiveUp;
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    let two_three = Tile::all()
        .into_iter()
        .find(|tile| (tile.a, tile.b) == (2, 3))
        .expect("2-3 tile");
    let double_two = Tile::all()
        .into_iter()
        .find(|tile| (tile.a, tile.b) == (2, 2))
        .expect("2-2 tile");
    state.hands = HashMap::from([
        (0, vec![two_three, double_two]),
        (1, vec![Tile::from_id(27).expect("6-6 tile")]),
        (2, vec![Tile::from_id(22).expect("4-4 tile")]),
        (3, vec![Tile::from_id(12).expect("1-6 tile")]),
    ]);
    let (_, play_score, _) = state
        .play_tile(0, two_three.id, None)
        .expect("five-up play");
    assert_eq!(play_score, 5);
    assert_eq!(state.scores[&0], 5);

    state.current_position = 0;
    let (_, _, result) = state
        .play_tile(0, double_two.id, Some(state.endpoints[0].endpoint_id))
        .expect("winning double two");
    let result = result.expect("round result");
    assert_eq!(result.round_score, (12 + 8 + 7) / 5);
    assert_eq!(state.scores[&0], 10);
}

#[test]
fn keep_drawing_requires_the_boneyard_to_be_exhausted_before_pass() {
    let mut state = state();
    state.no_playable_tiles = DominoesNoPlayableTiles::KeepDrawing;
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.endpoints = vec![super::Endpoint {
        endpoint_id: 1,
        placement_id: 0,
        pip: 6,
        port: share_type_public::DominoesPort::Right,
        anchor_x: 2,
        anchor_y: 0,
        direction: share_type_public::DominoesPort::Right,
    }];
    state.hands = HashMap::from([(0, vec![Tile::from_id(0).expect("0-0 tile")])]);
    state.boneyard = vec![Tile::from_id(1).expect("0-1 tile")];
    assert_eq!(state.pass(0), Err(super::CoreError::PassNotAllowed));
    let drawn = state.draw_tile(0).expect("draw unmatched tile");
    assert!(!drawn.playable);
    assert!(!drawn.passed);
    assert!(state.boneyard.is_empty());
    assert!(state.pass(0).expect("pass after boneyard").is_none());
}

#[test]
fn draw_one_automatically_passes_an_unplayable_draw() {
    let mut state = state();
    state.no_playable_tiles = DominoesNoPlayableTiles::DrawOne;
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.endpoints = vec![super::Endpoint {
        endpoint_id: 1,
        placement_id: 0,
        pip: 6,
        port: share_type_public::DominoesPort::Right,
        anchor_x: 2,
        anchor_y: 0,
        direction: share_type_public::DominoesPort::Right,
    }];
    state.hands = HashMap::from([(0, vec![Tile::from_id(0).expect("0-0 tile")])]);
    state.boneyard = vec![Tile::from_id(1).expect("0-1 tile")];
    let drawn = state.draw_tile(0).expect("draw-one action");
    assert!(drawn.passed);
    assert!(!drawn.playable);
    assert_eq!(state.current_position, 1);
}

#[test]
fn a_block_is_awarded_to_the_last_player_who_placed_a_tile() {
    let mut state = state();
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.last_play_position = Some(3);
    state.consecutive_passes = 3;
    state.endpoints = vec![super::Endpoint {
        endpoint_id: 1,
        placement_id: 0,
        pip: 6,
        port: share_type_public::DominoesPort::Right,
        anchor_x: 2,
        anchor_y: 0,
        direction: share_type_public::DominoesPort::Right,
    }];
    state.hands = HashMap::from([
        (0, vec![Tile::from_id(0).expect("0-0 tile")]),
        (1, vec![Tile::from_id(1).expect("0-1 tile")]),
        (2, vec![Tile::from_id(2).expect("0-2 tile")]),
        (3, vec![Tile::from_id(3).expect("0-3 tile")]),
    ]);
    let result = state.pass(0).expect("fourth pass").expect("blocked round");
    assert!(result.blocked);
    assert_eq!(result.winner_position, 3);
}

#[test]
fn doubles_create_four_open_ends_and_three_when_attached() {
    let mut state = state();
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    let double = Tile::from_id(18).expect("3-3 tile");
    let attach = Tile::all()
        .into_iter()
        .find(|tile| tile.a == 3 && tile.b == 4)
        .expect("3-4 tile");
    state.hands = HashMap::from([
        (0, vec![double, Tile::from_id(0).expect("0-0 tile")]),
        (1, vec![attach]),
    ]);
    let (placement, _, _) = state.play_tile(0, double.id, None).expect("first double");
    assert_eq!(placement.new_endpoints.len(), 4);
    state.current_position = 1;
    let endpoint = state.endpoints[0].endpoint_id;
    state
        .play_tile(1, attach.id, Some(endpoint))
        .expect("attach tile");
    assert_eq!(state.endpoints.len(), 4);
}

#[test]
fn layout_keeps_all_twenty_eight_tiles_non_overlapping() {
    let mut state = state();
    state.phase = DominoesPhase::Play;
    state.hands = state
        .positions
        .iter()
        .map(|position| (*position, Tile::all()))
        .collect();
    for _ in 0..super::TILE_COUNT {
        let position = state.current_position;
        let legal = state.legal_plays(position);
        let selected = legal.first().copied().expect("a legal stress play");
        state
            .play_tile(position, selected.tile_id, selected.endpoint_id)
            .expect("stress play");
    }

    for (index, placement) in state.placements.iter().enumerate() {
        let (width, height) = super::half_extents(placement.orientation);
        for other in state.placements.iter().skip(index + 1) {
            let (other_width, other_height) = super::half_extents(other.orientation);
            assert!(
                !((placement.center_x - other.center_x).abs() < width + other_width
                    && (placement.center_y - other.center_y).abs() < height + other_height)
            );
        }
    }
}

#[test]
fn simple_round_scores_opponents_remaining_pips() {
    let mut state = state();
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    let winner_tile = Tile::from_id(0).expect("0-0 tile");
    state.hands = HashMap::from([
        (0, vec![winner_tile]),
        (1, vec![Tile::from_id(27).expect("6-6 tile")]),
        (2, vec![Tile::from_id(1).expect("0-1 tile")]),
        (3, vec![Tile::from_id(2).expect("0-2 tile")]),
    ]);
    let (_, score, result) = state
        .play_tile(0, winner_tile.id, None)
        .expect("winning play");
    assert_eq!(score, 0);
    let result = result.expect("round result");
    assert_eq!(result.round_score, 15);
    assert_eq!(state.scores[&0], 15);
    assert_eq!(state.phase, DominoesPhase::RoundOver);
}
