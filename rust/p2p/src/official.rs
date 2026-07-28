use std::sync::Arc;

use share_type_public::GameId;

use crate::runtime::{P2pRuntimeOptions, P2pTurnPermissionChecker};

pub(crate) fn runtime_options() -> P2pRuntimeOptions {
    let checker: P2pTurnPermissionChecker = Arc::new(|session_id| {
        Box::pin(async move {
            let Some(session_id) = session_id.filter(|session_id| !session_id.is_empty()) else {
                return false;
            };
            match data::game_room_authorization(&session_id, GameId::ALL).await {
                Ok(authorization) => authorization.has_active_membership,
                Err(error) => {
                    eprintln!("[p2p][official] TURN permission lookup failed: {error}");
                    false
                }
            }
        })
    });
    P2pRuntimeOptions::turn_permission_gated(checker)
}
