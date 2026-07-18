#![cfg(feature = "official")]

use landlord::game::LandlordGameHandler;
use share_type_public::GameId;

type OfficialGameHandler = LandlordGameHandler;
const OFFICIAL_GAME_ID: GameId = GameId::LANDLORD;
const OFFICIAL_SERVICE_NAME: &str = "landlord";

include!("../../official_membership_e2e.rs");
