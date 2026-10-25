use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use share_type_public::{DominoesNoPlayableTiles, DominoesPhase, DominoesPort, DominoesRule};
use ws_common::CommonGameState;

use super::{DominoesRoundState, Tile};

#[path = "tests/layout_sequences.rs"]
mod layout_sequences;
#[path = "tests/legal_plays.rs"]
mod legal_plays;
#[path = "tests/public_history.rs"]
mod public_history;
#[path = "tests/round_order.rs"]
mod round_order;
#[path = "tests/scoring.rs"]
mod scoring;
#[path = "tests/starter_order.rs"]
mod starter_order;
#[path = "tests/switch_replay.rs"]
mod switch_replay;

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

fn tile(a: i32, b: i32) -> Tile {
    Tile::all()
        .into_iter()
        .find(|candidate| candidate.a == a.min(b) && candidate.b == a.max(b))
        .expect("tile exists")
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
fn starter_candidates_return_to_the_complete_deck_and_can_change_players() {
    // Switch 实机 sm42780597：01:51.76 CPU 选中 0-4，02:01.767
    // 正式发牌后 0-4 出现在玩家手中，选牌不是保留在选择者手里的起手牌。
    // 录像只直接确认牌会回流，不证明原作内部采用哪一种洗牌算法。
    for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
        for player_count in [3, 4] {
            let mut state = state_with_players(player_count);
            state.rule = rule;
            state.rng_state = 1;
            state.start_new_game_pending().expect("pending start");
            let candidates = state.starter_pool.clone();
            assert_eq!(candidates.len(), 6);
            let mut candidate_ids = candidates.iter().map(|tile| tile.id).collect::<Vec<_>>();
            candidate_ids.sort_unstable();
            candidate_ids.dedup();
            assert_eq!(candidate_ids.len(), 6);

            for position in 0..player_count {
                let result = state
                    .select_starter_slot(position, position)
                    .expect("select a distinct slot");
                if position + 1 < player_count {
                    assert_eq!(result.starter_position, None);
                    assert!(state.hands.is_empty());
                    assert!(state.boneyard.is_empty());
                } else {
                    assert!(
                        state
                            .positions
                            .contains(&result.starter_position.expect("starter"))
                    );
                }
            }

            assert_eq!(state.phase, DominoesPhase::Play);
            assert_eq!(state.hands.len(), player_count);
            assert!(state.hands.values().all(|hand| hand.len() == 5));
            assert_eq!(state.boneyard.len(), 28 - player_count * 5);
            let mut dealt_ids = state
                .hands
                .values()
                .flatten()
                .chain(&state.boneyard)
                .map(|tile| tile.id)
                .collect::<Vec<_>>();
            dealt_ids.sort_unstable();
            assert_eq!(dealt_ids, (0..28).collect::<Vec<_>>());
            for candidate in &candidates {
                assert_eq!(
                    dealt_ids.iter().filter(|id| **id == candidate.id).count(),
                    1
                );
            }
            assert!(candidates.iter().take(player_count).enumerate().any(
                |(selector, candidate)| {
                    state
                        .hands
                        .iter()
                        .any(|(position, hand)| *position != selector && hand.contains(candidate))
                }
            ));
        }
    }
}

#[test]
fn pending_starter_selection_reveals_only_chosen_slots_and_deals_after_all_choices() {
    let mut state = state_with_players(3);
    state.start_new_game_pending().expect("pending start");
    assert_eq!(state.phase, DominoesPhase::StarterSelection);
    assert_eq!(state.starter_pool.len(), 6);
    assert!(state.hands.is_empty());
    assert!(
        state
            .starter_selection_event()
            .expect("selection event")
            .slots
            .iter()
            .all(|slot| slot.tile.is_none() && slot.selected_by.is_none())
    );

    assert_eq!(
        state.select_starter_slot(1, 0),
        Err(super::CoreError::NotYourTurn)
    );
    let first = state.select_starter_slot(0, 0).expect("first choice");
    assert_eq!(first.starter_position, None);
    let event = state.starter_selection_event().expect("selection event");
    assert_eq!(event.slots[0].tile, Some(state.starter_pool[0].into()));
    assert_eq!(event.slots[0].selected_by, Some(0));
    assert!(event.slots[1..].iter().all(|slot| slot.tile.is_none()));
    assert!(state.select_starter_slot(0, 1).is_err());

    state.select_starter_slot(1, 1).expect("second choice");
    let last = state.select_starter_slot(2, 2).expect("third choice");
    assert!(last.starter_position.is_some());
    assert_eq!(state.phase, DominoesPhase::Play);
    assert!(state.hands.values().all(|hand| hand.len() == 5));
}

#[test]
fn automatic_starter_selection_does_not_read_hidden_pips_or_reuse_slots() {
    for player_count in [3, 4] {
        for seed in 1..=32 {
            let mut state = state_with_players(player_count);
            state.rng_state = seed;
            state.start_new_game_pending().expect("selection");
            for position in 0..player_count {
                let mut swapped = state.clone();
                swapped.starter_pool.reverse();
                let slot = state.automatic_starter_slot().expect("available slot");
                assert_eq!(swapped.automatic_starter_slot(), Some(slot));
                assert!(!state.starter_selections.values().any(|used| *used == slot));
                state
                    .select_starter_slot(position, slot)
                    .expect("automatic choice");
            }
            assert_eq!(state.phase, DominoesPhase::Play);
            assert_eq!(state.automatic_starter_slot(), None);
        }
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
    assert_eq!(result.score_changes[&0], 10);
    assert_eq!(result.score_changes[&1], 0);
}

#[test]
fn keep_drawing_requires_all_drawable_tiles_before_pass_and_preserves_two_reserved_tiles() {
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
    state.boneyard = vec![tile(5, 6), tile(6, 6), tile(0, 1)];
    assert_eq!(state.pass(0), Err(super::CoreError::PassNotAllowed));
    let drawn = state.draw_tile(0).expect("draw unmatched tile");
    assert!(!drawn.playable);
    assert!(!drawn.passed);
    assert_eq!(drawn.tile, Some(tile(0, 1)));
    assert_eq!(state.boneyard, [tile(5, 6), tile(6, 6)]);
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
    state.boneyard = vec![tile(5, 6), tile(6, 6), tile(0, 1)];
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
fn simple_doubles_keep_only_the_two_chain_ends() {
    let mut state = state();
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    let three_five = Tile::all()
        .into_iter()
        .find(|tile| (tile.a, tile.b) == (3, 5))
        .expect("3-5 tile");
    let double_five = Tile::all()
        .into_iter()
        .find(|tile| (tile.a, tile.b) == (5, 5))
        .expect("5-5 tile");
    let five_six = Tile::all()
        .into_iter()
        .find(|tile| (tile.a, tile.b) == (5, 6))
        .expect("5-6 tile");
    state.hands = HashMap::from([
        (0, vec![three_five, Tile::from_id(0).expect("0-0 tile")]),
        (1, vec![double_five, Tile::from_id(1).expect("0-1 tile")]),
        (2, vec![five_six, Tile::from_id(2).expect("0-2 tile")]),
        (3, vec![Tile::from_id(3).expect("0-3 tile")]),
    ]);
    state.play_tile(0, three_five.id, None).expect("play 3-5");
    let five_endpoint = state
        .endpoints
        .iter()
        .find(|endpoint| endpoint.pip == 5)
        .expect("right-side five")
        .endpoint_id;
    let (double_placement, _, _) = state
        .play_tile(1, double_five.id, Some(five_endpoint))
        .expect("play vertical 5-5");
    assert_eq!(
        double_placement.orientation,
        share_type_public::DominoesOrientation::Vertical
    );
    assert_eq!(state.endpoints.len(), 2);
    assert_eq!(double_placement.new_endpoints.len(), 1);
    assert_eq!(double_placement.new_endpoints[0].port, DominoesPort::Right);
    let continued_five = state
        .endpoints
        .iter()
        .find(|endpoint| endpoint.placement_id == double_placement.placement_id)
        .expect("double keeps the outward chain end")
        .endpoint_id;
    state
        .play_tile(2, five_six.id, Some(continued_five))
        .expect("play 5-6 after visual crosspiece");
    assert_eq!(state.endpoints.len(), 2);
}

#[test]
fn five_up_doubles_are_spinners_with_extra_open_ends() {
    let mut state = state();
    state.rule = DominoesRule::FiveUp;
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    let double = Tile::from_id(18).expect("3-3 tile");
    let attach = Tile::all()
        .into_iter()
        .find(|tile| tile.a == 3 && tile.b == 4)
        .expect("3-4 tile");
    state.hands = HashMap::from([
        (0, vec![double, Tile::from_id(0).expect("0-0 tile")]),
        (1, vec![attach, Tile::from_id(1).expect("0-1 tile")]),
    ]);
    let (placement, _, _) = state.play_tile(0, double.id, None).expect("first double");
    assert_eq!(placement.new_endpoints.len(), 4);
    let endpoint = state.endpoints[0].endpoint_id;
    state
        .play_tile(1, attach.id, Some(endpoint))
        .expect("attach tile");
    assert_eq!(state.endpoints.len(), 4);
}

#[test]
fn opening_doubles_have_two_simple_ends_and_four_five_up_branches() {
    for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
        let mut state = state();
        state.rule = rule;
        state.phase = DominoesPhase::Play;
        state.current_position = 0;
        let double_four = Tile::from_id(22).expect("4-4 tile");
        state.hands = HashMap::from([
            (0, vec![double_four, Tile::from_id(0).expect("0-0 tile")]),
            (1, vec![Tile::from_id(1).expect("0-1 tile")]),
            (2, vec![Tile::from_id(2).expect("0-2 tile")]),
            (3, vec![Tile::from_id(3).expect("0-3 tile")]),
        ]);

        let (placement, _, _) = state
            .play_tile(0, double_four.id, None)
            .expect("opening double");

        let count = if rule == DominoesRule::Simple { 2 } else { 4 };
        assert_eq!(placement.new_endpoints.len(), count, "{rule:?}");
        assert_eq!(state.endpoints.len(), count, "{rule:?}");
        let mut ports = state
            .endpoints
            .iter()
            .map(|endpoint| endpoint.port as i32)
            .collect::<Vec<_>>();
        ports.sort_unstable();
        let expected = if rule == DominoesRule::Simple {
            vec![DominoesPort::Left as i32, DominoesPort::Right as i32]
        } else {
            vec![
                DominoesPort::Left as i32,
                DominoesPort::Right as i32,
                DominoesPort::Top as i32,
                DominoesPort::Bottom as i32,
            ]
        };
        assert_eq!(ports, expected, "{rule:?} opening ports");
    }
}

#[test]
fn five_up_preview_score_matches_the_selected_spinner_endpoint() {
    let mut state = state();
    state.rule = DominoesRule::FiveUp;
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    let opening = Tile::from_id(5).expect("0-5 tile");
    let double_five = Tile::from_id(25).expect("5-5 tile");
    state.hands = HashMap::from([
        (0, vec![opening, Tile::from_id(0).expect("0-0 tile")]),
        (1, vec![double_five]),
        (2, vec![Tile::from_id(1).expect("0-1 tile")]),
        (3, vec![Tile::from_id(2).expect("0-2 tile")]),
    ]);
    state.play_tile(0, opening.id, None).expect("opening five");
    state.current_position = 1;

    let five_endpoint = state
        .endpoints
        .iter()
        .find(|endpoint| endpoint.pip == 5)
        .copied()
        .expect("five endpoint");
    let legal = state
        .legal_plays(1)
        .into_iter()
        .find(|play| play.endpoint_id == Some(five_endpoint.endpoint_id))
        .expect("double five legal play");
    assert_eq!(legal.score, 10);

    let (_, score, _) = state
        .play_tile(1, double_five.id, Some(five_endpoint.endpoint_id))
        .expect("spinner endpoint");
    assert_eq!(score, 10);
}

#[test]
fn every_spinner_branch_accepts_a_matching_tile_and_preserves_other_ends() {
    for opening_double in [true, false] {
        for port in [
            DominoesPort::Left,
            DominoesPort::Right,
            DominoesPort::Top,
            DominoesPort::Bottom,
        ] {
            if !opening_double && port == DominoesPort::Left {
                continue; // 后续双牌的左侧已经连接 3-5。
            }
            let mut state = state();
            state.rule = DominoesRule::FiveUp;
            state.phase = DominoesPhase::Play;
            state.current_position = 0;
            let attacher = if opening_double { 1 } else { 2 };
            state.hands = if opening_double {
                HashMap::from([
                    (
                        0,
                        vec![Tile::from_id(25).unwrap(), Tile::from_id(0).unwrap()],
                    ),
                    (
                        1,
                        vec![Tile::from_id(26).unwrap(), Tile::from_id(1).unwrap()],
                    ),
                    (2, vec![Tile::from_id(2).unwrap()]),
                    (3, vec![Tile::from_id(4).unwrap()]),
                ])
            } else {
                HashMap::from([
                    (
                        0,
                        vec![Tile::from_id(20).unwrap(), Tile::from_id(0).unwrap()],
                    ),
                    (
                        1,
                        vec![Tile::from_id(25).unwrap(), Tile::from_id(1).unwrap()],
                    ),
                    (
                        2,
                        vec![Tile::from_id(26).unwrap(), Tile::from_id(2).unwrap()],
                    ),
                    (3, vec![Tile::from_id(4).unwrap()]),
                ])
            };
            if opening_double {
                state.play_tile(0, 25, None).expect("opening 5-5");
            } else {
                state.play_tile(0, 20, None).expect("opening 3-5");
                let end = state
                    .endpoints
                    .iter()
                    .find(|end| end.pip == 5)
                    .unwrap()
                    .endpoint_id;
                state.play_tile(1, 25, Some(end)).expect("attach 5-5");
            }
            let spinner = state.placements.last().unwrap().placement_id;
            let endpoint = *state
                .endpoints
                .iter()
                .find(|end| end.placement_id == spinner && end.port == port)
                .unwrap();
            let previous = state.table_snapshot().endpoints;
            let legal = state
                .legal_plays(attacher)
                .into_iter()
                .find(|play| play.tile_id == 26 && play.endpoint_id == Some(endpoint.endpoint_id))
                .expect("5-6 is legal on this branch");
            let (placement, score, result) = state
                .play_tile(attacher, 26, legal.endpoint_id)
                .expect("play 5-6 on the selected branch");
            assert!(result.is_none());
            assert_eq!(placement.connected_endpoint_id, Some(endpoint.endpoint_id));
            assert_eq!(score, legal.score);
            assert_eq!(placement.new_endpoints.len(), 1);
            assert_eq!(placement.new_endpoints[0].pip, 6);
            let inward_pip = match endpoint.direction {
                DominoesPort::Left | DominoesPort::Top => {
                    if placement.flipped {
                        placement.tile.a
                    } else {
                        placement.tile.b
                    }
                }
                DominoesPort::Right | DominoesPort::Bottom => {
                    if placement.flipped {
                        placement.tile.b
                    } else {
                        placement.tile.a
                    }
                }
            };
            assert_eq!(inward_pip, 5, "{opening_double} {port:?}");
            let after = state.table_snapshot().endpoints;
            assert_eq!(after.len(), previous.len());
            assert!(
                !after
                    .iter()
                    .any(|end| end.endpoint_id == endpoint.endpoint_id)
            );
            for untouched in previous
                .iter()
                .filter(|end| end.endpoint_id != endpoint.endpoint_id)
            {
                assert!(
                    after.contains(untouched),
                    "unselected end changed: {untouched:?}"
                );
            }
        }
    }
}

#[test]
fn layout_keeps_all_twenty_eight_tiles_non_overlapping() {
    // double-six 图的欧拉路径使用每张牌恰好一次，也包含全部七种双牌。
    fn walk(pip: i32, unused: &mut Vec<Tile>, chain: &mut Vec<Tile>) {
        while let Some(index) = unused
            .iter()
            .position(|tile| tile.a == pip || tile.b == pip)
        {
            let tile = unused.remove(index);
            walk(if tile.a == pip { tile.b } else { tile.a }, unused, chain);
            chain.push(tile);
        }
    }
    let mut chain = Vec::new();
    walk(0, &mut Tile::all(), &mut chain);
    chain.reverse();
    for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
        let mut state = state();
        state.rule = rule;
        state.phase = DominoesPhase::Play;
        state.hands = state
            .positions
            .iter()
            .map(|position| (*position, Tile::all()))
            .collect();
        let mut endpoint = None;
        let mut folded = false;
        for tile in &chain {
            let position = state.current_position;
            let previous_direction = state
                .endpoints
                .iter()
                .find(|end| Some(end.endpoint_id) == endpoint)
                .map(|end| end.direction);
            let selected = state
                .legal_plays(position)
                .into_iter()
                .find(|play| play.tile_id == tile.id && play.endpoint_id == endpoint)
                .expect("the next distinct tile follows the chain");
            let (placement, _, _) = state
                .play_tile(position, selected.tile_id, selected.endpoint_id)
                .expect("stress play");
            if !tile.is_double() && previous_direction.is_some() {
                folded |= previous_direction != Some(placement.new_endpoints[0].direction);
            }
            endpoint = Some(placement.new_endpoints[0].endpoint_id);
            // 这是纯布局压力场景：让任一轮到的座位能放下一张指定牌，
            // 并从所有座位移除已摆出的牌，避免只重复测试同一张双空白。
            for hand in state.hands.values_mut() {
                hand.retain(|remaining| remaining.id != tile.id);
            }
        }

        assert_eq!(state.placements.len(), super::TILE_COUNT);
        // 同一长分支必须折返；Five-Up 不能因为有 spinner 就一直向外铺成直线。
        assert!(folded, "{rule:?}: the full chain never folds");
        assert_eq!(
            state
                .placements
                .iter()
                .map(|placement| placement.tile.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            super::TILE_COUNT
        );

        for (index, placement) in state.placements.iter().enumerate() {
            let (width, height) = super::half_extents(placement.orientation);
            for other in state.placements.iter().skip(index + 1) {
                let (other_width, other_height) = super::half_extents(other.orientation);
                assert!(
                    !((placement.center_x - other.center_x).abs() < width + other_width
                        && (placement.center_y - other.center_y).abs() < height + other_height),
                    "{rule:?}: placements {} {:?} {:?} at ({}, {}) and {} {:?} {:?} at ({}, {}) overlap",
                    placement.placement_id,
                    placement.tile,
                    placement.orientation,
                    placement.center_x,
                    placement.center_y,
                    other.placement_id,
                    other.tile,
                    other.orientation,
                    other.center_x,
                    other.center_y,
                );
            }
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
    assert_eq!(result.score_changes[&0], 15);
    assert_eq!(result.score_changes[&1], 0);
    assert_eq!(state.phase, DominoesPhase::RoundOver);
    assert_eq!(state.last_round_result.as_ref(), Some(&result));
}

#[test]
fn round_result_is_kept_for_rejoin_until_the_next_round_starts() {
    let mut state = state();
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.scores.insert(0, 34);
    let winner_tile = Tile::from_id(0).expect("0-0 tile");
    state.hands = HashMap::from([
        (0, vec![winner_tile]),
        (1, vec![Tile::from_id(27).expect("6-6 tile")]),
        (2, vec![Tile::from_id(1).expect("0-1 tile")]),
        (3, vec![Tile::from_id(2).expect("0-2 tile")]),
    ]);

    let (_, _, result) = state
        .play_tile(0, winner_tile.id, None)
        .expect("winning play");
    let result = result.expect("round result");
    assert!(result.game_over);
    assert_eq!(state.phase, DominoesPhase::GameOver);
    assert_eq!(state.last_round_result.as_ref(), Some(&result));

    state.phase = DominoesPhase::RoundOver;
    state.start_next_round().expect("next round");
    assert!(state.last_round_result.is_none());
}

#[test]
fn game_over_winners_are_all_players_with_the_highest_final_score() {
    let mut state = state();
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.scores = HashMap::from([(0, 34), (1, 34), (2, 35), (3, 10)]);
    let winner_tile = Tile::from_id(0).expect("0-0 tile");
    state.hands = HashMap::from([
        (0, vec![winner_tile]),
        (1, vec![Tile::from_id(1).expect("0-1 tile")]),
        (2, vec![]),
        (3, vec![]),
    ]);

    let (_, _, result) = state
        .play_tile(0, winner_tile.id, None)
        .expect("finish final round");
    let result = result.expect("game result");
    assert!(result.game_over);
    assert_eq!(result.scores[&0], 35);
    assert_eq!(result.winner_positions, vec![0, 2]);
}
