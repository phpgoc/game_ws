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
pub fn create_match(room_service: &mut RoomService, room_key: &str) {
    use std::collections::HashMap;

    use share_type_public::GameId;

    room_service.spawn_room_official_match(room_key, move |sessions, password| async move {
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
    }, |game_match_id| async move {
        if let Err(error) = data::service::game_match::finish(game_match_id).await {
            ws_common::dlog!(ws_common::tracing::Level::ERROR, "official match cleanup failed: {}", error);
        }
    });
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
    round: i32,
) {
    let official_match = {
        let room = room_service.lock().await;
        let Some(current_common) = room.room_common_state(room_key) else {
            return;
        };
        if !std::sync::Arc::ptr_eq(&current_common, expected_common) {
            return;
        }
        room.room_official_match(room_key)
    };
    let (Some(official_match), Some(landlord_position)) = (official_match, landlord_position)
    else {
        return;
    };

    tokio::spawn(async move {
        let Some(info) = official_match.wait().await else {
            return;
        };
        let game_match_id = info.id;
        let landlord_user_id = info.user_id(landlord_position).unwrap_or(0);
        if let Err(err) = data::service::game_round::landlord_settlement(
            data::input::GameRoundLandlordSettleInput {
                round,
                game_match_id,
                landlord_user_id,
                landlord_win,
                score: i64::from(score.max(1)),
            },
        )
        .await
        {
            ws_common::dlog!(
                ws_common::tracing::Level::ERROR,
                "[landlord][official] round stats failed: {}",
                err
            );
        }
    });
}

#[cfg(not(feature = "official"))]
pub async fn settle_round(
    _room_service: &tokio::sync::Mutex<RoomService>,
    _room_key: &str,
    _expected_common: &std::sync::Arc<std::sync::Mutex<ws_common::CommonGameState>>,
    _landlord_position: Option<usize>,
    _landlord_win: bool,
    _score: u32,
    _round: i32,
) {
}
