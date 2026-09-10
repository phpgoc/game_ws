use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::{player_scores_for_official_users, winner_scores_for_settlement};
use crate::game::{build_settlement_event_with_configs, settlement_score_changes_for_state};
use crate::game_state::{ShenyangMahjongLoopState, build_meld};
use crate::rules::ShenyangMahjongWinContext;
use share_type_public::games::shenyang_mahjong::{
    ShenyangMahjongMeldKind, WsShenyangMahjongScoreChange,
};
use ws_common::CommonGameState;

#[test]
fn official_winner_scores_record_every_discard_winner() {
    let base = Arc::new(Mutex::new(CommonGameState::default()));
    for position in 0..4 {
        base.lock()
            .unwrap()
            .add_player(position, position as u64 + 1, &format!("P{position}"));
    }
    let mut state = ShenyangMahjongLoopState::new(base);
    state.dealer_position = 0;
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
    state
        .hands
        .insert(2, vec![1, 2, 14, 15, 16, 24, 25, 26, 35, 35]);
    for (position, tile) in [(1, 31), (2, 32)] {
        state.melds.insert(
            position,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![tile; 3],
                Some(3),
            )],
        );
    }
    state.enter_settlement(vec![1, 2], Some(0), Some(3), false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let configs = HashMap::new();
    let score_changes =
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs);

    // 平胡基础 0 番，边张、付款人坐庄及闭门各 +1，分别付 2^3 = 8。
    // 两名赢家时不成立三家闭；未点炮的座位 3 不付款。
    assert_eq!(
        score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -16), (1, 8), (2, 8), (3, 0)],
    );
    let winner_scores = winner_scores_for_settlement(
        &state,
        settlement,
        &score_changes,
        ShenyangMahjongWinContext::new(),
        |position| Some(position as i32 + 10),
    );
    let official_scores = winner_scores
        .iter()
        .map(|winner| (winner.winner_user_id, winner.score))
        .collect::<Vec<_>>();
    assert_eq!(official_scores, vec![(11, 8), (12, 8)]);
    assert!(winner_scores.iter().all(|winner| {
        winner.pattern == data::payload::ShenyangMahjongRoundWinPattern::Standard
    }));

    let event = build_settlement_event_with_configs(&state, &configs).expect("settlement event");
    assert_eq!(event.winner_positions, vec![1, 2]);
    assert_eq!(
        event
            .winner_details
            .iter()
            .map(|winner| (winner.position + 10, i64::from(winner.score)))
            .collect::<Vec<_>>(),
        official_scores,
    );
}

#[test]
fn official_player_scores_keep_human_losses_when_ai_wins() {
    let scores = player_scores_for_official_users(
        &[
            WsShenyangMahjongScoreChange {
                position: 0,
                score: -12,
            },
            WsShenyangMahjongScoreChange {
                position: 1,
                score: 12,
            },
            WsShenyangMahjongScoreChange {
                position: 2,
                score: 0,
            },
        ],
        |position| match position {
            0 => Some(101),
            2 => Some(103),
            _ => None,
        },
    );

    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0].user_id, 101);
    assert_eq!(scores[0].score, -12);
    assert_eq!(scores[1].user_id, 103);
    assert_eq!(scores[1].score, 0);
}
