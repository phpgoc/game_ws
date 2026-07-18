#![cfg(feature = "official")]

use share_type_public::GameId;
use shenyang_mahjong::game::ShenyangMahjongGameHandler;

type OfficialGameHandler = ShenyangMahjongGameHandler;
const OFFICIAL_GAME_ID: GameId = GameId::SHENYANG_MAHJONG;
const OFFICIAL_SERVICE_NAME: &str = "shenyang-mahjong";

include!("../../official_membership_e2e.rs");
