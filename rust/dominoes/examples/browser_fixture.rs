//! 导出来自真实洗牌、合法出牌的多分支后期局面，供 Web 浏览器回归使用。
//! cargo run -p dominoes --no-default-features --example browser_fixture > fixture.json
use std::sync::{Arc, Mutex};

use dominoes::core::DominoesRoundState;
use serde_json::{Value, json};
use share_type_public::{DominoesNoPlayableTiles, DominoesPhase, DominoesRule};
use ws_common::CommonGameState;

fn main() {
    for seed in 1..=10_000 {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("P{position}"));
        }
        let mut state = DominoesRoundState::new_with_seed(
            Arc::new(Mutex::new(common)),
            DominoesRule::FiveUp,
            DominoesNoPlayableTiles::KeepDrawing,
            61,
            seed,
        )
        .expect("four-player round");
        state.start_new_game().expect("deal");
        let mut choices = seed * 7919;
        let mut actions = Vec::<Value>::new();
        for _ in 0..256 {
            if state.phase != DominoesPhase::Play {
                break;
            }
            let position = state.current_position;
            let legal = state.legal_plays(position);
            if legal.is_empty() {
                let hand = state.hand_state(position);
                if hand.can_draw {
                    state.draw_tile(position).expect("legal draw");
                    actions.push(json!({ "position": position, "draw": true }));
                } else {
                    state.pass(position).expect("legal pass");
                    actions.push(json!({ "position": position, "pass": true }));
                }
                continue;
            }
            let multi = legal.iter().find(|play| {
                legal
                    .iter()
                    .filter(|other| other.tile_id == play.tile_id)
                    .count()
                    >= 3
                    && legal
                        .iter()
                        .any(|other| other.tile_id == play.tile_id && other.score > 0)
            });
            if state.placements.len() >= 20 && state.endpoints.len() >= 8 && multi.is_some() {
                let tile_id = multi.unwrap().tile_id;
                let before =
                    json!({ "table": state.table_snapshot(), "hand": state.hand_state(position) });
                let outcomes = legal.iter().filter(|play| play.tile_id == tile_id).map(|play| {
                    let mut next = state.clone();
                    next.play_tile(position, tile_id, play.endpoint_id).expect("branch play");
                    json!({ "play": play, "table": next.table_snapshot(), "hand": next.hand_state(position) })
                }).collect::<Vec<_>>();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "seed": seed, "self_position": position, "actions": actions,
                        "before": before, "outcomes": outcomes,
                    }))
                    .expect("fixture JSON")
                );
                return;
            }
            choices ^= choices << 13;
            choices ^= choices >> 7;
            choices ^= choices << 17;
            let play = legal[choices as usize % legal.len()];
            state
                .play_tile(position, play.tile_id, play.endpoint_id)
                .expect("legal play");
            actions.push(json!({ "position": position, "play": play }));
        }
    }
    panic!("no late multi-branch fixture found in bounded seed search");
}
