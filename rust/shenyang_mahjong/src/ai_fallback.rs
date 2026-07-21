use std::collections::HashMap;

use ws_common::{Dispatch, RoomService};

use crate::game_state::ShenyangMahjongLoopState;

pub fn maybe_play_ai_turn(
    _room_service: &RoomService,
    _room_key: &str,
    _state: &mut ShenyangMahjongLoopState,
    _configs: &HashMap<String, i32>,
    _dispatch: &mut Dispatch,
) -> bool {
    false
}

pub fn maybe_resolve_ai_claims(
    _room_service: &RoomService,
    _room_key: &str,
    _state: &mut ShenyangMahjongLoopState,
    _configs: &HashMap<String, i32>,
    _dispatch: &mut Dispatch,
) -> bool {
    false
}
