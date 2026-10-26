use std::collections::HashMap;

use share_type_public::GameId;
use ws_common::RoomService;

use share_type_public::WsTexasHoldEmSettlementEvent;

#[cfg(feature = "official")]
pub fn create_match(room_service: &mut RoomService, room_key: &str, game_id: GameId) {
    use std::collections::HashMap;

    // Only the standard Texas Hold'em table is an official game.  The open,
    // short-deck and Omaha variants remain custom WS games and must not create
    // official match/stat records.
    if game_id != GameId::TEXAS_HOLD_EM {
        return;
    }

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

        match data::service::game_match::create(data::input::GameMatchCreateInput {
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
    }, |game_match_id| async move {
        if let Err(error) = data::service::game_match::finish(game_match_id).await {
            ws_common::dlog!(ws_common::tracing::Level::ERROR, "official match cleanup failed: {}", error);
        }
    });
}

#[cfg(not(feature = "official"))]
pub fn create_match(_room_service: &mut RoomService, _room_key: &str, _game_id: GameId) {}

#[cfg(feature = "official")]
pub fn settle_round(
    room_service: &RoomService,
    room_key: &str,
    settlement: &WsTexasHoldEmSettlementEvent,
    starting_chips: &HashMap<usize, i32>,
    round: i32,
) -> Option<tokio::task::JoinHandle<()>> {
    let Some(official_match) = room_service.room_official_match(room_key) else {
        return None;
    };
    let settlement = settlement.clone();
    let starting_chips = starting_chips.clone();
    Some(tokio::spawn(async move {
        let Some(info) = official_match.wait().await else {
            return;
        };
        let game_match_id = info.id;
        let player_scores = settlement
            .players
            .iter()
            .filter_map(|player| {
                let position = usize::try_from(player.position).ok()?;
                let user_id = info.user_id(position)?;
                let starting = starting_chips.get(&position).copied().unwrap_or_default();
                Some(data::input::GameRoundHoldemPlayerScoreInput {
                    user_id,
                    score: i64::from(player.chips.saturating_sub(starting)),
                })
            })
            .collect::<Vec<_>>();
        if player_scores.is_empty() {
            return;
        }

        let input = data::input::GameRoundHoldemSettleInput {
            round,
            game_match_id,
            player_scores,
        };
        if let Err(err) = data::service::game_round::holdem_settlement(input).await {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[holdem][official] round stats failed: {}",
                err
            );
        }
    }))
}

#[cfg(not(feature = "official"))]
pub fn settle_round(
    _room_service: &RoomService,
    _room_key: &str,
    _settlement: &WsTexasHoldEmSettlementEvent,
    _starting_chips: &HashMap<usize, i32>,
    _round: i32,
) -> Option<tokio::task::JoinHandle<()>> {
    None
}

#[cfg(all(test, feature = "official"))]
mod tests {
    use super::*;
    use crate::game::HoldemGameHandler;
    use share_type_public::{Routes, WsJoinRequest, WsTexasHoldEmSettlementPlayer};
    use ws_common::{ClientRequest, GameHandler};

    fn join_request(name: &str, official_session_id: String) -> ClientRequest {
        ClientRequest {
            route: Routes::JOIN as i32,
            data: serde_json::to_value(WsJoinRequest {
                request_id: None,
                name: name.to_owned(),
                password: "official-holdem-room".to_owned(),
                game_id: GameId::TEXAS_HOLD_EM,
                session_id: official_session_id,
                avatar_url: String::new(),
            })
            .expect("serialize join request"),
        }
    }

    fn settled_player(position: i32, chips: i32) -> WsTexasHoldEmSettlementPlayer {
        WsTexasHoldEmSettlementPlayer {
            position,
            name: format!("player-{position}"),
            cards: Vec::new(),
            open_cards: Vec::new(),
            folded: false,
            chips,
            starting_chips: 1000,
            chip_delta: chips - 1000,
            hand_rank: 0,
            hand_name: String::new(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn standard_table_creates_match_without_publishing_official_stats() {
        let temp_dir = std::env::temp_dir().join(format!(
            "lan-game-official-holdem-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp data directory");
        let db_path = temp_dir.join("official-holdem.data");
        data::init_with_config(data::config::DataConfig::sqlite_file(
            db_path.to_string_lossy().as_ref(),
        ))
        .await
        .expect("initialize data store");

        let first = data::service::user::create(data::input::UserCreateInput {
            name: "holdem-winner".to_owned(),
            account: "holdem-winner-account".to_owned(),
            email: None,
            third_platform: 2,
            avatar_url: "https://example.com/winner.png".to_owned(),
            share_id: 0,
        })
        .await
        .expect("create first user");
        let second = data::service::user::create(data::input::UserCreateInput {
            name: "holdem-loser".to_owned(),
            account: "holdem-loser-account".to_owned(),
            email: None,
            third_platform: 2,
            avatar_url: "https://example.com/loser.png".to_owned(),
            share_id: 0,
        })
        .await
        .expect("create second user");
        let first_session = data::service::cache::set_session(first.id)
            .await
            .expect("create first session");
        let second_session = data::service::cache::set_session(second.id)
            .await
            .expect("create second session");

        let handler = HoldemGameHandler::default();
        let mut room = RoomService::default();
        for (connection_id, name, session) in [
            (1, "holdem-winner", first_session),
            (2, "holdem-loser", second_session),
        ] {
            room.handle_common_request(
                connection_id,
                &join_request(name, session),
                handler.game_id(),
                || handler.build_room_settings(),
            )
            .expect("handle official join");
        }

        create_match(&mut room, "official-holdem-room", GameId::OMAHA_HOLD_EM);
        assert_eq!(room.room_official_match_id("official-holdem-room"), None);

        create_match(&mut room, "official-holdem-room", GameId::TEXAS_HOLD_EM);
        room.room_official_match("official-holdem-room")
            .unwrap()
            .wait()
            .await
            .unwrap();
        assert!(
            room.room_official_match_id("official-holdem-room")
                .is_some()
        );

        settle_round(
            &room,
            "official-holdem-room",
            &WsTexasHoldEmSettlementEvent {
                winners: vec![0],
                pot: 500,
                public_cards: Vec::new(),
                players: vec![settled_player(0, 1250), settled_player(1, 750)],
                next_hand_countdown: 0,
            },
            &[(0, 1000), (1, 1000)].into_iter().collect(),
            1,
        )
        .unwrap()
        .await
        .unwrap();

        let first_stats =
            data::repository::game_user_stats::get_by_user_game(first.id, GameId::TEXAS_HOLD_EM)
                .await
                .expect("read first user stats");
        let second_stats =
            data::repository::game_user_stats::get_by_user_game(second.id, GameId::TEXAS_HOLD_EM)
                .await
                .expect("read second user stats");

        assert_eq!(first_stats.match_count, 0);
        assert_eq!(first_stats.round_count, 0);
        assert_eq!(first_stats.win_count, 0);
        assert_eq!(first_stats.lose_count, 0);
        assert_eq!(first_stats.draw_count, 0);
        assert_eq!(first_stats.win_score, 0);
        assert_eq!(first_stats.lose_score, 0);
        assert_eq!(second_stats.match_count, 0);
        assert_eq!(second_stats.round_count, 0);
        assert_eq!(second_stats.win_count, 0);
        assert_eq!(second_stats.lose_count, 0);
        assert_eq!(second_stats.draw_count, 0);
        assert_eq!(second_stats.win_score, 0);
        assert_eq!(second_stats.lose_score, 0);

        data::shutdown().await;
        std::fs::remove_dir_all(temp_dir).expect("remove temp data directory");
    }
}
