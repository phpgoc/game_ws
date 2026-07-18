#![cfg(feature = "official")]

use share_type_public::GameId;
use tractor::game::TractorGameHandler;

type OfficialGameHandler = TractorGameHandler;
const OFFICIAL_GAME_ID: GameId = GameId::TRACTOR;
const OFFICIAL_SERVICE_NAME: &str = "tractor";

include!("../../official_membership_e2e.rs");
