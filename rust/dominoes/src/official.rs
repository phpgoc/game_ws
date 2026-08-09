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
