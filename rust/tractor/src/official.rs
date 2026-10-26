use share_type_public::{TractorRank, TractorSuit};
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
    match data::service::game_room::authorize(&session_id, GameId::TRACTOR).await {
        Ok(authorization) => ws_common::JoinAuthorization {
            can_create_room: authorization.can_create_room,
            has_active_membership: authorization.has_active_membership,
        },
        Err(err) => {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[tractor][official] join authorization failed: {}",
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
                        "[tractor][official] skip match stats: invalid session at position {}: {}",
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
            game_id: GameId::TRACTOR,
            password,
            user_ids,
        })
        .await
        {
            Ok(created) => Some((created.game_match.id, user_ids_by_position)),
            Err(err) => {
                ws_common::dlog!(
                    ws_common::tracing::Level::ERROR,
                    "[tractor][official] create match stats failed: {}",
                    err
                );
                None
            }
        }
    });
}

#[cfg(not(feature = "official"))]
pub fn create_match(_room_service: &mut RoomService, _room_key: &str) {}

#[cfg(feature = "official")]
pub fn settle_round(
    room_service: &RoomService,
    room_key: &str,
    winner_positions: &[i32],
    score: i32,
    target_rank: TractorRank,
    trump_suit: Option<TractorSuit>,
    round: i32,
) {
    let Some(official_match) = room_service.room_official_match(room_key) else {
        return;
    };
    if winner_positions.len() < 2 {
        return;
    }
    let winners = [winner_positions[0] as usize, winner_positions[1] as usize];
    let score = score.max(1);

    tokio::spawn(async move {
        let Some(info) = official_match.wait().await else {
            return;
        };
        let game_match_id = info.id;
        // AI 胜方没有用户账号；0 只标识 AI，不会写入真人统计。
        let winner_user_id_1 = info.user_id(winners[0]).unwrap_or(0);
        let winner_user_id_2 = info.user_id(winners[1]).unwrap_or(0);
        if let Err(err) = data::service::game_round::tractor_settlement(
            data::input::GameRoundTractorSettleInput {
                round,
                game_match_id,
                winner_user_id_1,
                winner_user_id_2,
                score: i64::from(score),
                target_rank: target_rank as i32,
                trump_suit: trump_suit.map(|suit| suit as i32),
            },
        )
        .await
        {
            ws_common::dlog!(
                ws_common::tracing::Level::ERROR,
                "[tractor][official] round stats failed: {}",
                err
            );
        }
    });
}

#[cfg(not(feature = "official"))]
pub fn settle_round(
    _room_service: &RoomService,
    _room_key: &str,
    _winner_positions: &[i32],
    _score: i32,
    _target_rank: TractorRank,
    _trump_suit: Option<TractorSuit>,
    _round: i32,
) {
}
