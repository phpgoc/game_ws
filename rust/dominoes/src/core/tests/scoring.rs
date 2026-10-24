use super::super::{Endpoint, Placement};
use super::*;
use share_type_public::{DominoesOrientation, DominoesPort};

fn tile(a: i32, b: i32) -> Tile {
    Tile::all()
        .into_iter()
        .find(|tile| tile.a == a.min(b) && tile.b == a.max(b))
        .unwrap()
}

#[test]
fn spinner_counts_twice_until_its_second_connection_then_only_the_branches_score() {
    let mut state = state();
    state.rule = DominoesRule::FiveUp;
    state.phase = DominoesPhase::Play;
    state.hands.insert(
        0,
        vec![
            tile(5, 5),
            tile(0, 5),
            tile(1, 5),
            tile(4, 5),
            tile(5, 6),
            tile(0, 0),
        ],
    );
    let mut ports = Vec::new();
    for ((a, b), expected_score) in [
        ((5, 5), 10),
        ((0, 5), 10),
        ((1, 5), 0),
        ((4, 5), 5),
        ((5, 6), 0),
    ] {
        state.current_position = 0;
        let endpoint_id = if ports.is_empty() {
            None
        } else {
            Some(ports.remove(0))
        };
        let predicted = state
            .legal_plays(0)
            .into_iter()
            .find(|play| play.tile_id == tile(a, b).id && play.endpoint_id == endpoint_id)
            .unwrap();
        assert_eq!(predicted.score, expected_score);
        let (placement, score, _) = state.play_tile(0, predicted.tile_id, endpoint_id).unwrap();
        assert_eq!(score, expected_score);
        if endpoint_id.is_none() {
            ports = placement
                .new_endpoints
                .iter()
                .map(|end| end.endpoint_id)
                .collect();
        }
    }
    assert_eq!(state.endpoints.len(), 4);
    assert!(state.endpoints.iter().all(|end| end.placement_id != 0));
}

#[test]
fn switch_screenshot_forecast_is_thirty_from_two_terminal_doubles_and_two_plain_ends() {
    // 原作公开截图 20240102004653.jpg：4-4、6-6 两张端部双牌，
    // 另两端是 4 和 1；把 1-6 接到 1，原作预告 30 = 8 + 12 + 4 + 6。
    // 来源：https://amesss.hatenablog.com/entry/2024/01/02/120000
    let mut state = state();
    state.rule = DominoesRule::FiveUp;
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.hands.insert(0, vec![tile(1, 6), tile(0, 0)]);
    for (placement_id, tile, count) in [
        (0, tile(4, 4), 3),
        (1, tile(6, 6), 3),
        (2, tile(4, 5), 1),
        (3, tile(0, 1), 1),
    ] {
        let pip = match placement_id {
            0 => 4,
            1 => 6,
            2 => 4,
            _ => 1,
        };
        let new_endpoints = (0..count)
            .map(|port| Endpoint {
                endpoint_id: placement_id * 4 + port,
                placement_id,
                pip,
                port: DominoesPort::Right,
                direction: DominoesPort::Right,
                anchor_x: placement_id * 6 + 2,
                anchor_y: port * 6,
            })
            .collect::<Vec<_>>();
        state.endpoints.extend(new_endpoints.iter().copied());
        state.placements.push(Placement {
            placement_id,
            tile,
            connected_endpoint_id: Some(100 + placement_id),
            connected_port: Some(DominoesPort::Left),
            center_x: placement_id * 6,
            center_y: 0,
            orientation: DominoesOrientation::Horizontal,
            flipped: false,
            new_endpoints,
        });
    }
    state.next_placement_id = 4;
    state.next_endpoint_id = 16;
    let play = state
        .legal_plays(0)
        .into_iter()
        .find(|play| play.tile_id == tile(1, 6).id && play.endpoint_id == Some(12))
        .unwrap();
    assert_eq!(play.score, 30);
    let (_, score, _) = state.play_tile(0, play.tile_id, play.endpoint_id).unwrap();
    assert_eq!(score, 30);
}

// 独立按落牌连接图数邻居，不使用 BoardEnds 或剩余端口计数。
fn score_from_placement_graph(state: &DominoesRoundState) -> i32 {
    let mut total = 0;
    for placement in &state.placements {
        if placement.tile.is_double() {
            let degree = usize::from(placement.connected_endpoint_id.is_some())
                + state
                    .placements
                    .iter()
                    .filter(|other| {
                        other.connected_endpoint_id.is_some_and(|id| {
                            placement
                                .new_endpoints
                                .iter()
                                .any(|end| end.endpoint_id == id)
                        })
                    })
                    .count();
            if degree <= 1 {
                total += placement.tile.pip_sum();
            }
        } else {
            total += state
                .endpoints
                .iter()
                .filter(|end| end.placement_id == placement.placement_id)
                .map(|end| end.pip)
                .sum::<i32>();
        }
    }
    if total % 5 == 0 { total } else { 0 }
}

#[test]
fn real_spinner_forecasts_match_independent_connection_graph_scoring() {
    let mut checked = 0;
    let mut distinct_same_pip_scores = 0;
    for players in [3, 4] {
        for seed in 1..=24 {
            let mut state = state_with_players(players);
            state.rule = DominoesRule::FiveUp;
            state.no_playable_tiles = DominoesNoPlayableTiles::KeepDrawing;
            state.rng_state = seed;
            state.start_new_game().unwrap();
            for turn in 0..200 {
                if state.phase != DominoesPhase::Play {
                    break;
                }
                let position = state.current_position;
                let legal = state.legal_plays(position);
                for play in &legal {
                    let mut actual = state.clone();
                    let (_, score, _) = actual
                        .play_tile(position, play.tile_id, play.endpoint_id)
                        .unwrap();
                    assert_eq!(score, score_from_placement_graph(&actual));
                    assert_eq!(play.score, score);
                    checked += 1;
                    if legal.iter().any(|other| {
                        other.tile_id == play.tile_id
                            && other.score != play.score
                            && other
                                .endpoint_id
                                .and_then(|id| state.board_end(id))
                                .zip(play.endpoint_id.and_then(|id| state.board_end(id)))
                                .is_some_and(|(a, b)| a.pip == b.pip && a.spinner != b.spinner)
                    }) {
                        distinct_same_pip_scores += 1;
                    }
                }
                if legal.is_empty() {
                    state.draw_tile(position).unwrap();
                } else {
                    let play = legal[(turn + seed as usize) % legal.len()];
                    state
                        .play_tile(position, play.tile_id, play.endpoint_id)
                        .unwrap();
                }
            }
            assert_ne!(state.phase, DominoesPhase::Play);
        }
    }
    assert!(checked > 1_000);
    assert!(
        distinct_same_pip_scores > 0,
        "same pip can have different scoring topology"
    );
}
