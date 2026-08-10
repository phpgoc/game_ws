use crate::core::RoundResult;
use share_type_public::DominoesRule;
use ws_common::RoomService;

#[cfg(feature = "official")]
pub async fn authorize_join(session_id: String) -> ws_common::JoinAuthorization {
    use share_type_public::GameId;
    if session_id.is_empty() {
        return ws_common::JoinAuthorization {
            can_create_room: false,
            has_active_membership: false,
        };
    }
    match data::service::game_room::authorize(&session_id, GameId::DOMINOES).await {
        Ok(authorization) => ws_common::JoinAuthorization {
            can_create_room: authorization.can_create_room,
            has_active_membership: authorization.has_active_membership,
        },
        Err(error) => {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[dominoes][official] join authorization failed: {}",
                error
            );
            ws_common::JoinAuthorization {
                can_create_room: false,
                has_active_membership: false,
            }
        }
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
            match data::service::cache::get_session(&player.session_id).await {
                Ok(user) => {
                    user_ids.push(user.id);
                    user_ids_by_position.insert(player.position, user.id);
                }
                Err(error) => {
                    ws_common::dlog!(
                        ws_common::tracing::Level::WARN,
                        "[dominoes][official] skip match stats: invalid session at position {}: {}",
                        player.position,
                        error
                    );
                    return None;
                }
            }
        }

        let own_user_id = user_ids_by_position
            .get(&0)
            .copied()
            .or(user_ids.first().copied())?;
        match data::service::game_match::create(data::input::GameMatchCreateInput {
            own_user_id,
            game_id: GameId::DOMINOES,
            password,
            user_ids,
        })
        .await
        {
            Ok(created) => Some((created.game_match.id, user_ids_by_position)),
            Err(error) => {
                ws_common::dlog!(
                    ws_common::tracing::Level::ERROR,
                    "[dominoes][official] create match stats failed: {}",
                    error
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
fn player_scores_for_official_users<F>(
    result: &RoundResult,
    mut user_id_for_position: F,
) -> Vec<data::input::GameRoundDominoesPlayerScoreInput>
where
    F: FnMut(usize) -> Option<i32>,
{
    let mut score_changes = result.score_changes.iter().collect::<Vec<_>>();
    score_changes.sort_unstable_by_key(|(position, _)| **position);
    score_changes
        .into_iter()
        .filter_map(|(position, score)| {
            Some(data::input::GameRoundDominoesPlayerScoreInput {
                user_id: user_id_for_position(*position)?,
                score: i64::from((*score).max(0)),
                is_winner: *position == result.winner_position,
            })
        })
        .collect()
}

#[cfg(feature = "official")]
pub fn settle_round(
    room_service: &RoomService,
    room_key: &str,
    round: i32,
    rule: DominoesRule,
    result: &RoundResult,
) {
    let Some(game_match_id) = room_service.room_official_match_id(room_key) else {
        return;
    };
    let winner_user_id = room_service.room_official_user_id(room_key, result.winner_position);
    let player_scores = player_scores_for_official_users(result, |position| {
        room_service.room_official_user_id(room_key, position)
    });
    if player_scores.is_empty() {
        return;
    }
    let blocked = result.blocked;
    let round_score = i64::from(result.round_score.max(0));

    tokio::spawn(async move {
        if let Err(error) = data::service::game_round::dominoes_settlement(
            data::input::GameRoundDominoesSettleInput {
                game_match_id,
                round,
                winner_user_id,
                rule,
                blocked,
                round_score,
                player_scores,
            },
        )
        .await
        {
            ws_common::dlog!(
                ws_common::tracing::Level::ERROR,
                "[dominoes][official] round stats failed: {}",
                error
            );
        }
    });
}

#[cfg(not(feature = "official"))]
pub fn settle_round(
    _room_service: &RoomService,
    _room_key: &str,
    _round: i32,
    _rule: DominoesRule,
    _result: &RoundResult,
) {
}

#[cfg(all(test, feature = "official"))]
#[path = "official/tests.rs"]
mod tests;
