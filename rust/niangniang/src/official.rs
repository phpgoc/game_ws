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
    match data::service::game_room::authorize(&session_id, GameId::NIANGNIANG).await {
        Ok(authorization) => ws_common::JoinAuthorization {
            can_create_room: authorization.can_create_room,
            has_active_membership: authorization.has_active_membership,
        },
        Err(error) => {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[niangniang][official] join authorization failed: {}",
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
                        "[niangniang][official] skip match stats: invalid session at position {}: {}",
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
            game_id: GameId::NIANGNIANG,
            password,
            user_ids,
        })
        .await
        {
            Ok(created) => Some((created.game_match.id, user_ids_by_position)),
            Err(error) => {
                ws_common::dlog!(
                    ws_common::tracing::Level::ERROR,
                    "[niangniang][official] create match stats failed: {}",
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
pub fn settle_round(
    service: &RoomService,
    key: &str,
    state: &share_type_public::games::niangniang::NiangniangStateEvent,
) {
    let Some(game_match_id) = service.room_official_match_id(key) else {
        return;
    };
    let Some(winner) = state.match_winner else {
        return;
    };
    let scores = state
        .players
        .iter()
        .filter_map(|p| {
            Some((
                service.room_official_user_id(key, p.position as usize)?,
                if p.position % 2 == winner { 1 } else { -1 },
            ))
        })
        .collect::<Vec<_>>();
    tokio::spawn(async move {
        if let Err(error) = data::service::niangniang::settle(game_match_id, scores).await {
            ws_common::dlog!(
                ws_common::tracing::Level::ERROR,
                "[niangniang] round stats failed: {}",
                error
            );
        }
    });
}
#[cfg(not(feature = "official"))]
pub fn settle_round(
    _service: &RoomService,
    _key: &str,
    _state: &share_type_public::games::niangniang::NiangniangStateEvent,
) {
}
