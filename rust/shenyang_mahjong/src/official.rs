use std::collections::HashMap;

#[cfg(feature = "official")]
use share_type_public::games::shenyang_mahjong::WsShenyangMahjongScoreChange;
use ws_common::RoomService;

#[cfg(feature = "official")]
use crate::game::{
    settlement_from_position, settlement_is_reverse_win, settlement_score_changes_for_state,
    winner_pattern_with_rule,
};
use crate::game_state::{SettlementState, ShenyangMahjongLoopState};
#[cfg(feature = "official")]
use crate::rules::win_rule_from_configs;

#[cfg(feature = "official")]
fn block_on_official<F>(future: F) -> Option<F::Output>
where
    F: std::future::Future,
{
    let handle = tokio::runtime::Handle::try_current().ok()?;
    Some(tokio::task::block_in_place(|| handle.block_on(future)))
}

#[cfg(feature = "official")]
pub fn create_match(room_service: &mut RoomService, room_key: &str) {
    use std::collections::HashMap;

    use share_type_public::GameId;

    if room_service.room_official_match_id(room_key).is_some() {
        return;
    }
    let password = room_key.to_owned();
    let sessions = room_service.room_official_player_sessions(room_key);
    if sessions.is_empty() {
        return;
    }

    let Some(result) = block_on_official(async move {
        let mut user_ids = Vec::with_capacity(sessions.len());
        let mut user_ids_by_position = HashMap::new();
        for player in sessions {
            match data::cache_get_session(&player.session_id).await {
                Ok(user) => {
                    user_ids.push(user.id);
                    user_ids_by_position.insert(player.position, user.id);
                }
                Err(err) => {
                    ws_common::dlog!(
                        ws_common::tracing::Level::WARN,
                        "[shenyang_mahjong][official] skip match stats: invalid session at position {}: {}",
                        player.position,
                        err
                    );
                    return None;
                }
            }
        }

        let Some(own_user_id) = user_ids_by_position
            .get(&0)
            .copied()
            .or(user_ids.first().copied())
        else {
            return None;
        };

        match data::game_match_create(data::GameMatchCreateInput {
            own_user_id,
            game_id: GameId::SHENYANG_MAHJONG,
            password,
            user_ids,
        })
        .await
        {
            Ok(created) => Some((created.game_match.id, user_ids_by_position)),
            Err(err) => {
                ws_common::dlog!(
                    ws_common::tracing::Level::WARN,
                    "[shenyang_mahjong][official] create match stats failed: {}",
                    err
                );
                None
            }
        }
    }) else {
        return;
    };
    if let Some((match_id, user_ids_by_position)) = result {
        room_service.set_room_official_match(room_key, match_id, user_ids_by_position);
    }
}

#[cfg(not(feature = "official"))]
pub fn create_match(_room_service: &mut RoomService, _room_key: &str) {}

#[cfg(feature = "official")]
fn official_from_position_for_settlement(settlement: &SettlementState) -> Option<usize> {
    settlement_from_position(settlement)
}

#[cfg(feature = "official")]
fn official_reverse_win_for_settlement(
    state: &ShenyangMahjongLoopState,
    settlement: &SettlementState,
) -> bool {
    settlement_is_reverse_win(state, settlement)
}

#[cfg(feature = "official")]
pub fn settle_round(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
) {
    let Some(settlement) = state.settlement.as_ref() else {
        return;
    };
    let Some(game_match_id) = room_service.room_official_match_id(room_key) else {
        return;
    };

    let discarder_user_id = official_from_position_for_settlement(settlement)
        .and_then(|position| room_service.room_official_user_id(room_key, position));
    let is_reverse_win = official_reverse_win_for_settlement(state, settlement);
    let players = state.players_snapshot();
    let positions = players.keys().copied().collect::<Vec<_>>();
    let score_changes = settlement_score_changes_for_state(state, &positions, settlement, configs);
    let win_rule = win_rule_from_configs(configs);
    let winner_scores =
        winner_scores_for_settlement(state, settlement, &score_changes, win_rule, |position| {
            room_service.room_official_user_id(room_key, position)
        });

    tokio::spawn(async move {
        if let Err(err) = data::game_round_shenyang_mahjong_settlement(
            data::GameRoundShenyangMahjongSettleInput {
                game_match_id,
                is_draw: winner_scores.is_empty(),
                discarder_user_id,
                is_reverse_win,
                winner_scores,
            },
        )
        .await
        {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[shenyang_mahjong][official] round stats failed: {}",
                err
            );
        }
    });
}

#[cfg(not(feature = "official"))]
pub fn settle_round(
    _room_service: &RoomService,
    _room_key: &str,
    _state: &ShenyangMahjongLoopState,
    _configs: &HashMap<String, i32>,
) {
}

#[cfg(feature = "official")]
fn winner_pattern_for_position(
    state: &ShenyangMahjongLoopState,
    settlement: &SettlementState,
    position: usize,
    win_rule: i32,
) -> data::ShenyangMahjongRoundWinPattern {
    let mut hand_tiles = state.hands.get(&position).cloned().unwrap_or_default();
    if !settlement.is_self_draw
        && let Some(tile) = settlement.win_tile
    {
        hand_tiles.push(tile);
        hand_tiles.sort_unstable();
    }
    let melds = state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]);
    match winner_pattern_with_rule(&hand_tiles, melds, win_rule) {
        share_type_public::games::shenyang_mahjong::ShenyangMahjongWinPattern::Standard => {
            data::ShenyangMahjongRoundWinPattern::Standard
        }
        share_type_public::games::shenyang_mahjong::ShenyangMahjongWinPattern::PiaoHu => {
            data::ShenyangMahjongRoundWinPattern::PiaoHu
        }
        share_type_public::games::shenyang_mahjong::ShenyangMahjongWinPattern::SevenPairs => {
            data::ShenyangMahjongRoundWinPattern::SevenPairs
        }
        share_type_public::games::shenyang_mahjong::ShenyangMahjongWinPattern::PureOneSuit => {
            data::ShenyangMahjongRoundWinPattern::PureOneSuit
        }
    }
}

#[allow(dead_code)]
pub(crate) fn winner_score_for_settlement(
    settlement: &SettlementState,
    player_count: usize,
    winner_position: usize,
) -> i64 {
    if settlement.winner_positions.is_empty()
        || !settlement.winner_positions.contains(&winner_position)
    {
        return 0;
    }
    if settlement.is_self_draw {
        player_count
            .saturating_sub(settlement.winner_positions.len())
            .max(1) as i64
    } else {
        1
    }
}

#[allow(dead_code)]
fn winner_score_from_changes(
    score_changes: &[share_type_public::games::shenyang_mahjong::WsShenyangMahjongScoreChange],
    winner_position: usize,
) -> i64 {
    score_changes
        .iter()
        .find(|change| change.position == winner_position as i32)
        .map(|change| i64::from(change.score.max(0)))
        .unwrap_or(0)
}

#[cfg(feature = "official")]
fn winner_scores_for_settlement<F>(
    state: &ShenyangMahjongLoopState,
    settlement: &SettlementState,
    score_changes: &[WsShenyangMahjongScoreChange],
    win_rule: i32,
    mut user_id_for_position: F,
) -> Vec<data::GameRoundShenyangMahjongWinnerScoreInput>
where
    F: FnMut(usize) -> Option<i64>,
{
    let mut winner_scores = Vec::new();
    for position in &settlement.winner_positions {
        let score = winner_score_from_changes(score_changes, *position);
        if score <= 0 {
            continue;
        }
        if let Some(winner_user_id) = user_id_for_position(*position) {
            winner_scores.push(data::GameRoundShenyangMahjongWinnerScoreInput {
                winner_user_id,
                score,
                pattern: winner_pattern_for_position(state, settlement, *position, win_rule),
            });
        }
    }
    winner_scores
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "official")]
    use std::sync::{Arc, Mutex};

    use crate::game_state::SettlementState;
    #[cfg(feature = "official")]
    use crate::game_state::{ShenyangMahjongLoopState, build_meld};

    #[cfg(feature = "official")]
    use share_type_public::games::shenyang_mahjong::ShenyangMahjongMeldKind;
    use share_type_public::games::shenyang_mahjong::WsShenyangMahjongScoreChange;
    #[cfg(feature = "official")]
    use ws_common::CommonGameState;

    #[cfg(feature = "official")]
    use super::{
        official_from_position_for_settlement, official_reverse_win_for_settlement,
        winner_pattern_for_position, winner_scores_for_settlement,
    };
    use super::{winner_score_for_settlement, winner_score_from_changes};

    #[test]
    fn discard_win_scores_one_per_winner() {
        let settlement = SettlementState {
            winner_positions: vec![0, 2],
            from_position: Some(1),
            win_tile: Some(3),
            is_self_draw: false,
            is_reverse_win: false,
            is_gang_draw: false,
            is_haidilao: false,
        };

        assert_eq!(winner_score_for_settlement(&settlement, 4, 0), 1);
        assert_eq!(winner_score_for_settlement(&settlement, 4, 2), 1);
    }

    #[cfg(feature = "official")]
    #[test]
    fn official_reverse_win_requires_open_peng_source_and_discard_context() {
        let state_without_source = state_with_players();
        let mut state_with_source = state_with_players();
        state_with_source.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![5, 5, 5],
                Some(2),
            )],
        );
        let valid_reverse_win = SettlementState {
            winner_positions: vec![1],
            from_position: Some(0),
            win_tile: Some(5),
            is_self_draw: false,
            is_reverse_win: true,
            is_gang_draw: false,
            is_haidilao: false,
        };
        let invalid_self_draw_flag = SettlementState {
            winner_positions: vec![1],
            from_position: Some(0),
            win_tile: Some(5),
            is_self_draw: true,
            is_reverse_win: true,
            is_gang_draw: false,
            is_haidilao: false,
        };

        assert!(!official_reverse_win_for_settlement(
            &state_without_source,
            &valid_reverse_win
        ));
        assert!(official_reverse_win_for_settlement(
            &state_with_source,
            &valid_reverse_win
        ));
        assert_eq!(
            official_from_position_for_settlement(&valid_reverse_win),
            Some(0)
        );
        assert!(!official_reverse_win_for_settlement(
            &state_with_source,
            &invalid_self_draw_flag
        ));
        assert_eq!(
            official_from_position_for_settlement(&invalid_self_draw_flag),
            None
        );
    }

    #[cfg(feature = "official")]
    #[test]
    fn official_winner_scores_skip_zero_score_winners() {
        let state = state_with_players();
        let settlement = SettlementState {
            winner_positions: vec![0, 1],
            from_position: Some(2),
            win_tile: Some(9),
            is_self_draw: false,
            is_reverse_win: false,
            is_gang_draw: false,
            is_haidilao: false,
        };
        let score_changes = vec![
            WsShenyangMahjongScoreChange {
                position: 0,
                score: 0,
            },
            WsShenyangMahjongScoreChange {
                position: 1,
                score: 5,
            },
            WsShenyangMahjongScoreChange {
                position: 2,
                score: -5,
            },
        ];

        let winner_scores = winner_scores_for_settlement(
            &state,
            &settlement,
            &score_changes,
            crate::rules::WIN_RULE_SHENYANG_BASIC,
            |position| Some(position as i64 + 10),
        );

        assert_eq!(winner_scores.len(), 1);
        assert_eq!(winner_scores[0].winner_user_id, 11);
        assert_eq!(winner_scores[0].score, 5);
    }

    #[test]
    fn self_draw_score_counts_each_loser() {
        let settlement = SettlementState {
            winner_positions: vec![2],
            from_position: None,
            win_tile: Some(3),
            is_self_draw: true,
            is_reverse_win: false,
            is_gang_draw: false,
            is_haidilao: false,
        };

        assert_eq!(winner_score_for_settlement(&settlement, 4, 2), 3);
    }

    #[cfg(feature = "official")]
    fn state_with_players() -> ShenyangMahjongLoopState {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("P{}", position));
            }
        }
        ShenyangMahjongLoopState::new(base)
    }

    #[cfg(feature = "official")]
    #[test]
    fn winner_pattern_reuses_settlement_patterns_for_official_stats() {
        let mut seven_pairs_state = state_with_players();
        seven_pairs_state
            .hands
            .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35]);
        seven_pairs_state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(35),
            false,
            false,
            false,
            false,
        );
        let seven_pairs_settlement = seven_pairs_state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_pattern_for_position(
                &seven_pairs_state,
                seven_pairs_settlement,
                1,
                crate::rules::WIN_RULE_SHENYANG_BASIC
            ),
            data::ShenyangMahjongRoundWinPattern::SevenPairs
        );

        let mut piao_state = state_with_players();
        piao_state.hands.insert(2, vec![35, 35]);
        piao_state.melds.insert(
            2,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(1)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        piao_state.enter_settlement_with_reverse_win(
            vec![2],
            None,
            Some(35),
            true,
            false,
            false,
            false,
        );
        let piao_settlement = piao_state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_pattern_for_position(
                &piao_state,
                piao_settlement,
                2,
                crate::rules::WIN_RULE_SHENYANG_BASIC
            ),
            data::ShenyangMahjongRoundWinPattern::PiaoHu
        );
    }

    #[cfg(feature = "official")]
    #[test]
    fn winner_pattern_uses_win_rule_for_closed_pure_one_suit() {
        let mut state = state_with_players();
        state
            .hands
            .insert(1, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(9),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_pattern_for_position(&state, settlement, 1, crate::rules::WIN_RULE_RELAXED),
            data::ShenyangMahjongRoundWinPattern::PureOneSuit
        );
        assert_eq!(
            winner_pattern_for_position(
                &state,
                settlement,
                1,
                crate::rules::WIN_RULE_SHENYANG_BASIC
            ),
            data::ShenyangMahjongRoundWinPattern::PureOneSuit
        );
    }

    #[test]
    fn winner_score_clamps_non_positive_changes_to_zero() {
        let score_changes = vec![
            WsShenyangMahjongScoreChange {
                position: 0,
                score: -3,
            },
            WsShenyangMahjongScoreChange {
                position: 1,
                score: 0,
            },
        ];

        assert_eq!(winner_score_from_changes(&score_changes, 0), 0);
        assert_eq!(winner_score_from_changes(&score_changes, 1), 0);
        assert_eq!(winner_score_from_changes(&score_changes, 2), 0);
    }

    #[test]
    fn winner_score_uses_actual_positive_score_change() {
        let score_changes = vec![
            WsShenyangMahjongScoreChange {
                position: 0,
                score: 0,
            },
            WsShenyangMahjongScoreChange {
                position: 1,
                score: 5,
            },
            WsShenyangMahjongScoreChange {
                position: 2,
                score: -5,
            },
        ];

        assert_eq!(winner_score_from_changes(&score_changes, 1), 5);
    }
}
