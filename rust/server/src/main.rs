use std::process;

use ws_common::{GameRegistry, run_game_registry_server_with_cli};

const SERVICE_NAME: &str = "ws";
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error:#}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    let mut registry = GameRegistry::new();
    registry.register(landlord::game::LandlordGameHandler::default())?;
    registry.register(shenyang_mahjong::game::ShenyangMahjongGameHandler::default())?;
    registry.register(holdem::game::HoldemGameHandler::default())?;
    registry.register(tractor::game::TractorGameHandler::default())?;
    registry.register(upgrade::game::UpgradeGameHandler::default())?;
    registry.register(dominoes::game::DominoesGameHandler::default())?;
    registry.register(who_is_landlord::WhoIsLandlordHandler::default())?;
    registry.register(niangniang::NiangniangHandler::default())?;
    run_game_registry_server_with_cli(SERVICE_NAME, IDLE_TIMEOUT, registry).await
}
