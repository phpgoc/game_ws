use crate::protocol::{ClientEvent, ServerEvent};

pub async fn handle_event(event: ClientEvent) -> anyhow::Result<Option<ServerEvent>> {
    let response = match event {
        ClientEvent::Ping { ts } => ServerEvent::Pong { ts },
        ClientEvent::JoinTable { table_id, user_id } => ServerEvent::Joined { table_id, user_id },
        ClientEvent::CallLandlord { score } => ServerEvent::LandlordCalled { score },
        ClientEvent::PlayCards { cards } => ServerEvent::CardsPlayed { cards },
        ClientEvent::Pass => ServerEvent::Passed,
    };

    Ok(Some(response))
}
