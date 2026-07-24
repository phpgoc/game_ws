use share_type_public::GameId;
use ws_common::RoomService;

use share_type_public::WsTexasHoldEmSettlementEvent;

#[cfg(feature = "official")]
fn block_on_official<F>(future: F) -> Option<F::Output>
where
    F: std::future::Future,
{
    let handle = tokio::runtime::Handle::try_current().ok()?;
    Some(tokio::task::block_in_place(|| handle.block_on(future)))
}

#[cfg(feature = "official")]
pub fn create_match(room_service: &mut RoomService, room_key: &str, game_id: GameId) {
    use std::collections::HashMap;

    // Only the standard Texas Hold'em table is an official game.  The open,
    // short-deck and Omaha variants remain custom WS games and must not create
    // official match/stat records.
    if game_id != GameId::TEXAS_HOLD_EM {
        return;
    }

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
                        "[holdem][official] skip match stats: invalid session at position {}: {}",
                        player.position,
                        err
                    );
                    return None;
                }
            }
        }

        let Some(own_user_id) = user_ids.first().copied() else {
            return None;
        };

        match data::game_match_create(data::GameMatchCreateInput {
            own_user_id,
            game_id,
            password,
            user_ids,
        })
        .await
        {
            Ok(created) => Some((created.game_match.id, user_ids_by_position)),
            Err(err) => {
                ws_common::dlog!(
                    ws_common::tracing::Level::WARN,
                    "[holdem][official] create match stats failed: {}",
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
pub fn create_match(_room_service: &mut RoomService, _room_key: &str, _game_id: GameId) {}

#[cfg(feature = "official")]
pub fn settle_round(
    room_service: &RoomService,
    room_key: &str,
    settlement: &WsTexasHoldEmSettlementEvent,
    initial_chips: i32,
) {
    let Some(game_match_id) = room_service.room_official_match_id(room_key) else {
        return;
    };
    let player_scores = settlement
        .players
        .iter()
        .filter_map(|player| {
            let position = usize::try_from(player.position).ok()?;
            let user_id = room_service.room_official_user_id(room_key, position)?;
            Some(data::GameRoundHoldemPlayerScoreInput {
                user_id,
                score: i64::from(player.chips.saturating_sub(initial_chips)),
            })
        })
        .collect::<Vec<_>>();
    if player_scores.is_empty() {
        return;
    }

    let input = data::GameRoundHoldemSettleInput {
        game_match_id,
        player_scores,
    };
    tokio::spawn(async move {
        if let Err(err) = data::game_round_holdem_settlement(input).await {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[holdem][official] round stats failed: {}",
                err
            );
        }
    });
}

#[cfg(not(feature = "official"))]
pub fn settle_round(
    _room_service: &RoomService,
    _room_key: &str,
    _settlement: &WsTexasHoldEmSettlementEvent,
    _initial_chips: i32,
) {
}
