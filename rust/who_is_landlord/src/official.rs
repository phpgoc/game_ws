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
    match data::service::game_room::authorize(&session_id, GameId::WHO_IS_LANDLORD).await {
        Ok(authorization) => ws_common::JoinAuthorization {
            can_create_room: authorization.can_create_room,
            has_active_membership: authorization.has_active_membership,
        },
        Err(error) => {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[who_is_landlord][official] join authorization failed: {}",
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
                Err(error) => {
                    ws_common::dlog!(
                        ws_common::tracing::Level::WARN,
                        "[who_is_landlord][official] skip match stats: invalid session at position {}: {}",
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
            game_id: GameId::WHO_IS_LANDLORD,
            password,
            user_ids,
        })
        .await
        {
            Ok(created) => Some((created.game_match.id, user_ids_by_position)),
            Err(error) => {
                ws_common::dlog!(
                    ws_common::tracing::Level::ERROR,
                    "[who_is_landlord][official] create match stats failed: {}",
                    error
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
pub fn settle_round(
    service: &RoomService,
    key: &str,
    state: &share_type_public::games::who_is_landlord::WhoIsLandlordStateEvent,
) {
    let Some(official_match) = service.room_official_match(key) else {
        return;
    };
    let Some(landlord_position) = state.landlord_position else {
        return;
    };
    let scores = state.scores.clone();
    tokio::spawn(async move {
        let Some(info) = official_match.wait().await else {
            return;
        };
        let game_match_id = info.id;
        let scores = scores
            .iter()
            .enumerate()
            .filter_map(|(position, score)| Some((info.user_id(position)?, i64::from(*score))))
            .collect::<Vec<_>>();
        if let Err(error) =
            data::service::who_is_landlord::settle(game_match_id, landlord_position, scores).await
        {
            ws_common::dlog!(
                ws_common::tracing::Level::ERROR,
                "[who_is_landlord] round stats failed: {}",
                error
            );
        }
    });
}
#[cfg(not(feature = "official"))]
pub fn settle_round(
    _service: &RoomService,
    _key: &str,
    _state: &share_type_public::games::who_is_landlord::WhoIsLandlordStateEvent,
) {
}
