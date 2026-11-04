use std::collections::HashMap;

use super::official_match_users;
use super::player_scores_for_official_users;
use crate::core::RoundResult;

fn result(winner_position: usize) -> RoundResult {
    RoundResult {
        winner_position,
        blocked: false,
        round_score: 5,
        scores: HashMap::from([(0, 15), (1, 10), (2, 20)]),
        score_changes: HashMap::from([(0, 15), (1, 10), (2, 20)]),
        remaining_hands: HashMap::new(),
        game_over: false,
        winner_positions: Vec::new(),
    }
}

#[test]
fn official_player_scores_keep_five_up_points_for_human_losers() {
    let scores = player_scores_for_official_users(&result(0), |position| match position {
        0 => Some(101),
        1 => Some(202),
        _ => None,
    });

    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0].user_id, 101);
    assert_eq!(scores[0].score, 15);
    assert!(scores[0].is_winner);
    assert_eq!(scores[1].user_id, 202);
    assert_eq!(scores[1].score, 10);
    assert!(!scores[1].is_winner);
}

#[test]
fn native_ai_winner_does_not_mark_any_human_as_winner() {
    let scores = player_scores_for_official_users(&result(2), |position| match position {
        0 => Some(101),
        1 => Some(202),
        _ => None,
    });

    assert!(scores.iter().all(|score| !score.is_winner));
}

#[test]
fn official_match_users_uses_join_authorized_ids_without_revalidating_sessions() {
    let players = vec![
        ws_common::OfficialPlayerSession {
            position: 0,
            session_id: "expired-or-replaced-session".to_owned(),
            user_id: Some(101),
        },
        ws_common::OfficialPlayerSession {
            position: 1,
            session_id: "another-session".to_owned(),
            user_id: Some(202),
        },
    ];

    let (user_ids, user_ids_by_position) = official_match_users(&players).unwrap();
    assert_eq!(user_ids, vec![101, 202]);
    assert_eq!(user_ids_by_position, HashMap::from([(0, 101), (1, 202)]));
}

#[test]
fn official_match_users_rejects_missing_authorized_identity() {
    let players = vec![ws_common::OfficialPlayerSession {
        position: 0,
        session_id: "owner-session".to_owned(),
        user_id: None,
    }];

    assert!(official_match_users(&players).is_none());
}
