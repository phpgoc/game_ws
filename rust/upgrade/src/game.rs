use share_type_public::{GameId, WsResponseCode};
use ws_common::{ClientRequest, Dispatch, GameHandler, RoomService, SessionId, SharedGameState};

use crate::game_setting::build_upgrade_settings;

/// 独立升级服务的房间处理器。
///
/// 当前只接通公共房间生命周期；发牌和出牌路由会在后续规则提交中逐步加入。
#[derive(Debug, Default)]
pub struct UpgradeGameHandler;

impl GameHandler for UpgradeGameHandler {
    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_upgrade_settings()
    }

    fn game_id(&self) -> GameId {
        GameId::UPGRADE
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        room_service.error_response(session_id, request.route, WsResponseCode::NOT_IN_RANGE)
    }
}

#[cfg(test)]
#[path = "game/tests.rs"]
mod tests;
