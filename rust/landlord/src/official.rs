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
    match data::service::game_room::authorize(&session_id, GameId::LANDLORD).await {
        Ok(authorization) => ws_common::JoinAuthorization {
            can_create_room: authorization.can_create_room,
            has_active_membership: authorization.has_active_membership,
        },
        Err(err) => {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[landlord][official] join authorization failed: {}",
                err
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
                Err(err) => {
                    ws_common::dlog!(
                        ws_common::tracing::Level::WARN,
                        "[landlord][official] skip match stats: invalid session at position {}: {}",
                        player.position,
                        err
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
            game_id: GameId::LANDLORD,
            password,
            user_ids,
        })
        .await
        {
            Ok(created) => Some((created.game_match.id, user_ids_by_position)),
            Err(err) => {
                ws_common::dlog!(
                    ws_common::tracing::Level::ERROR,
                    "[landlord][official] create match stats failed: {}",
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
pub async fn settle_round(
    room_service: &tokio::sync::Mutex<RoomService>,
    room_key: &str,
    expected_common: &std::sync::Arc<std::sync::Mutex<ws_common::CommonGameState>>,
    landlord_position: Option<usize>,
    landlord_win: bool,
    score: u32,
) {
    let (game_match_id, landlord_user_id) = {
        let room = room_service.lock().await;
        let Some(current_common) = room.room_common_state(room_key) else {
            return;
        };
        if !std::sync::Arc::ptr_eq(&current_common, expected_common) {
            return;
        }
        let Some(game_match_id) = room.room_official_match_id(room_key) else {
            return;
        };
        let Some(landlord_position) = landlord_position else {
            return;
        };
        let Some(landlord_user_id) = room.room_official_user_id(room_key, landlord_position) else {
            return;
        };
        (game_match_id, landlord_user_id)
    };

    if let Err(err) =
        data::service::game_round::landlord_settlement(data::input::GameRoundLandlordSettleInput {
            game_match_id,
            landlord_user_id,
            landlord_win,
            score: i64::from(score.max(1)),
        })
        .await
    {
        ws_common::dlog!(
            ws_common::tracing::Level::ERROR,
            "[landlord][official] round stats failed: {}",
            err
        );
    }
}

#[cfg(not(feature = "official"))]
pub async fn settle_round(
    _room_service: &tokio::sync::Mutex<RoomService>,
    _room_key: &str,
    _expected_common: &std::sync::Arc<std::sync::Mutex<ws_common::CommonGameState>>,
    _landlord_position: Option<usize>,
    _landlord_win: bool,
    _score: u32,
) {
}
