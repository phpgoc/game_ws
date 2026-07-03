use ws_common::RoomService;

use crate::game_state::{SettlementState, ShenyangMahjongLoopState};
#[cfg(feature = "official")]
use crate::rules::is_seven_pairs_win;

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
fn winner_pattern_for_position(
    state: &ShenyangMahjongLoopState,
    settlement: &SettlementState,
    position: usize,
) -> data::ShenyangMahjongRoundWinPattern {
    let mut hand_tiles = state.hands.get(&position).cloned().unwrap_or_default();
    if !settlement.is_self_draw
        && let Some(tile) = settlement.win_tile
    {
        hand_tiles.push(tile);
        hand_tiles.sort_unstable();
    }
    let meld_count = state.melds.get(&position).map(Vec::len).unwrap_or(0);
    if meld_count == 0 && is_seven_pairs_win(&hand_tiles) {
        data::ShenyangMahjongRoundWinPattern::SevenPairs
    } else {
        data::ShenyangMahjongRoundWinPattern::Standard
    }
}

#[cfg(feature = "official")]
pub fn settle_round(room_service: &RoomService, room_key: &str, state: &ShenyangMahjongLoopState) {
    let Some(settlement) = state.settlement.as_ref() else {
        return;
    };
    let Some(game_match_id) = room_service.room_official_match_id(room_key) else {
        return;
    };

    let discarder_user_id = settlement
        .from_position
        .and_then(|position| room_service.room_official_user_id(room_key, position));
    let is_reverse_win = settlement.is_reverse_win;
    let player_count = room_service.room_official_player_sessions(room_key).len();
    let mut winner_scores = Vec::new();
    for position in &settlement.winner_positions {
        if let Some(winner_user_id) = room_service.room_official_user_id(room_key, *position) {
            winner_scores.push(data::GameRoundShenyangMahjongWinnerScoreInput {
                winner_user_id,
                score: winner_score_for_settlement(settlement, player_count, *position),
                pattern: winner_pattern_for_position(state, settlement, *position),
            });
        }
    }

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
) {
}

#[cfg(test)]
mod tests {
    use crate::game_state::SettlementState;

    use super::winner_score_for_settlement;

    #[test]
    fn self_draw_score_counts_each_loser() {
        let settlement = SettlementState {
            winner_positions: vec![2],
            from_position: None,
            win_tile: Some(3),
            is_self_draw: true,
            is_reverse_win: false,
        };

        assert_eq!(winner_score_for_settlement(&settlement, 4, 2), 3);
    }

    #[test]
    fn discard_win_scores_one_per_winner() {
        let settlement = SettlementState {
            winner_positions: vec![0, 2],
            from_position: Some(1),
            win_tile: Some(3),
            is_self_draw: false,
            is_reverse_win: false,
        };

        assert_eq!(winner_score_for_settlement(&settlement, 4, 0), 1);
        assert_eq!(winner_score_for_settlement(&settlement, 4, 2), 1);
    }
}
