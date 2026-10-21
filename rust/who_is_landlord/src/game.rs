use crate::core::Room;
use share_type_public::games::who_is_landlord::*;
use share_type_public::{CommonEvent, GameId, Routes, WsCode, WsResponseCode};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{
    ClientRequest, CommonGameState, Delivery, Dispatch, GameHandler, GameSettings, GameState,
    OutboundPayload, RequestResponse, RoomService, SessionId, SessionSenders, SharedGameState,
};

type RoundHandle = Arc<Mutex<Round>>;
type Registry = Arc<Mutex<HashMap<String, RoundHandle>>>;
struct Round {
    core: Room,
    settled: bool,
    base: Arc<Mutex<CommonGameState>>,
}
struct RunningState {
    common: Arc<Mutex<CommonGameState>>,
}
impl GameState for RunningState {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.common)
    }
    fn can_accept_players(&self) -> bool {
        false
    }
}
#[derive(Default)]
pub struct WhoIsLandlordHandler {
    states: Registry,
    senders: Option<SessionSenders>,
    service: Option<Arc<AsyncMutex<RoomService>>>,
}
fn event<T: serde::Serialize>(dispatch: &mut Dispatch, recipient: SessionId, code: i32, value: T) {
    dispatch.messages.push(Delivery {
        recipient,
        payload: OutboundPayload::Event(CommonEvent {
            code,
            data: serde_json::to_value(value).expect("serializable game event"),
        }),
    });
}
fn snapshot(service: &RoomService, key: &str, round: &Round, dispatch: &mut Dispatch) {
    for position in 0..3 {
        let state = round.core.state_for(position);
        for recipient in service.connected_session_ids_for_position(key, position) {
            event(
                dispatch,
                recipient,
                WhoIsLandlordWsCode::STATE as i32,
                &state,
            );
        }
    }
}
fn settle(service: &RoomService, key: &str, round: &mut Round) {
    if round.core.phase == WhoIsLandlordPhase::SETTLEMENT && !round.settled {
        round.settled = true;
        crate::official::settle_round(service, key, &round.core.state_for(0));
    }
}
impl WhoIsLandlordHandler {
    fn prune(&self, service: &RoomService) {
        self.states.lock().unwrap().retain(|key, round| {
            let round = round.lock().unwrap();
            service.room_common_state(key).is_some_and(|base| {
                Arc::ptr_eq(&base, &round.base) && !base.lock().unwrap().stop_requested
            })
        });
    }
    fn start(&mut self, service: &mut RoomService, session: SessionId) -> Dispatch {
        self.prune(service);
        let route = Routes::START as i32;
        if service.session_position(session) != Some(0) {
            return service.error_response(session, route, WsResponseCode::NO_PERMISSION);
        }
        let Some(key) = service.room_key_of(session) else {
            return service.error_response(session, route, WsResponseCode::NOT_IN_RANGE);
        };
        if self
            .states
            .lock()
            .unwrap()
            .get(&key)
            .is_some_and(|r| r.lock().unwrap().core.phase != WhoIsLandlordPhase::SETTLEMENT)
        {
            return service.error_response(session, route, WsResponseCode::NO_PERMISSION);
        }
        let Some(common) = service.room_common_state(&key) else {
            return Dispatch::default();
        };
        {
            let base = common.lock().unwrap();
            if base.players.len() != 3
                || !(0..3).all(|p| base.players.contains_key(&p))
                || !base.disconnected_positions.is_empty()
                || !base.ai_positions.is_empty()
            {
                return service.error_response(session, route, WsResponseCode::NOT_IN_RANGE);
            }
        }
        service.clear_room_game_state(&key);
        // A fresh common handle prevents delayed work from a previous round touching this one.
        let common = service
            .reset_room_common_state_for_new_game(&key)
            .expect("existing room");
        let names = {
            let base = common.lock().unwrap();
            std::array::from_fn(|p| base.player_name(p))
        };
        let core = Room::new(key.clone(), names, &mut rand::rng());
        let round = Arc::new(Mutex::new(Round {
            core,
            settled: false,
            base: Arc::clone(&common),
        }));
        service.set_room_game_state(
            &key,
            Box::new(RunningState {
                common: Arc::clone(&common),
            }),
        );
        self.states
            .lock()
            .unwrap()
            .insert(key.clone(), Arc::clone(&round));
        crate::official::create_match(service, &key);
        let mut dispatch = Dispatch::default();
        service.push_ok_response(&mut dispatch, session, route);
        service.broadcast_connected(&key, WsCode::START as i32, (), &mut dispatch);
        {
            let mut round = round.lock().unwrap();
            round.core.sync_presence(&common.lock().unwrap());
            snapshot(service, &key, &round, &mut dispatch);
        }
        if let (Some(service), Some(senders)) = (&self.service, &self.senders) {
            start_loop(
                key,
                common,
                round,
                Arc::clone(service),
                Arc::clone(senders),
                Arc::clone(&self.states),
            );
        }
        dispatch
    }
}
impl GameHandler for WhoIsLandlordHandler {
    fn game_id(&self) -> GameId {
        GameId::WHO_IS_LANDLORD
    }
    fn build_game_state(&self) -> Box<dyn GameState> {
        Box::new(SharedGameState::new())
    }
    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        (GameSettings::new(3, 3), HashMap::new())
    }
    fn authorize_join(
        &self,
        join: &share_type_public::WsJoinRequest,
    ) -> ws_common::JoinAuthorizationFuture {
        #[cfg(feature = "official")]
        {
            Box::pin(crate::official::authorize_join(join.session_id.clone()))
        }
        #[cfg(not(feature = "official"))]
        {
            let _ = join;
            Box::pin(async { ws_common::JoinAuthorization::ALLOW_NONMEMBER })
        }
    }
    fn set_context(&mut self, senders: SessionSenders, service: Arc<AsyncMutex<RoomService>>) {
        self.senders = Some(senders);
        self.service = Some(service);
    }
    fn handle_game_request(
        &mut self,
        service: &mut RoomService,
        session: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        if request.route == Routes::START as i32 {
            return self.start(service, session);
        }
        if request.route != WhoIsLandlordRoutes::ACTION as i32 {
            return service.error_response(session, request.route, WsResponseCode::NOT_IN_RANGE);
        }
        let Some(key) = service.room_key_of(session) else {
            return service.error_response(session, request.route, WsResponseCode::NO_PERMISSION);
        };
        let Some(position) = service.session_position(session) else {
            return Dispatch::default();
        };
        let Ok(action) = serde_json::from_value::<WhoIsLandlordActionRequest>(request.data) else {
            return service.error_response(session, request.route, WsResponseCode::ERROR_FORMAT);
        };
        let states = self.states.lock().unwrap();
        let Some(round) = states.get(&key) else {
            return service.error_response(session, request.route, WsResponseCode::NOT_IN_RANGE);
        };
        let mut round = round.lock().unwrap();
        let Some(common) = service.room_common_state(&key) else {
            return Dispatch::default();
        };
        {
            let base = common.lock().unwrap();
            if base.paused || base.stop_requested {
                return service.error_response(
                    session,
                    request.route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            round.core.sync_presence(&base);
        }
        if !round.core.apply(position, action.action) {
            return service.error_response(session, request.route, WsResponseCode::NOT_IN_RANGE);
        }
        let mut dispatch = Dispatch::default();
        service.push_ok_response(&mut dispatch, session, request.route);
        settle(service, &key, &mut round);
        snapshot(service, &key, &round, &mut dispatch);
        dispatch
    }
    fn after_common_request(
        &mut self,
        service: &mut RoomService,
        session: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        // QUIT uses common stop_requested; clear the hand immediately so a new player can fill the room.
        let stopped = self
            .states
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(key, round)| {
                let round = round.lock().unwrap();
                round
                    .base
                    .lock()
                    .unwrap()
                    .stop_requested
                    .then_some((key.clone(), Arc::clone(&round.base)))
            })
            .collect::<Vec<_>>();
        for (key, base) in stopped {
            service.clear_room_game_state_if_same(&key, &base);
            service.broadcast_connected(&key, WsCode::GAME_OVER as i32, (), dispatch);
        }
        self.prune(service);
        let states = self.states.lock().unwrap();
        let Some(key) = service.room_key_of(session) else {
            return;
        };
        let Some(round) = states.get(&key) else {
            return;
        };
        if request.route == Routes::JOIN as i32 {
            let joined = dispatch.messages.iter().any(|m| m.recipient == session && matches!(&m.payload,
                OutboundPayload::Response(RequestResponse::WithData(r)) if r.route == Routes::JOIN as i32 && r.code as i32 == WsResponseCode::JOINED as i32));
            if !joined {
                return;
            }
        }
        let Some(base) = service.room_common_state(&key) else {
            return;
        };
        let mut round = round.lock().unwrap();
        round.core.sync_presence(&base.lock().unwrap());
        snapshot(service, &key, &round, dispatch);
    }
}
fn start_loop(
    key: String,
    common: Arc<Mutex<CommonGameState>>,
    round: RoundHandle,
    service: Arc<AsyncMutex<RoomService>>,
    senders: SessionSenders,
    states: Registry,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(350)).await;
            let mut service = service.lock().await;
            let same = service
                .room_common_state(&key)
                .is_some_and(|b| Arc::ptr_eq(&b, &common));
            if !same {
                break;
            }
            let (stop, paused) = {
                let base = common.lock().unwrap();
                (base.stop_requested, base.paused)
            };
            let mut dispatch = Dispatch::default();
            if stop {
                service.clear_room_game_state_if_same(&key, &common);
                service.broadcast_connected(&key, WsCode::GAME_OVER as i32, (), &mut dispatch);
            } else if !paused {
                let mut round = round.lock().unwrap();
                let presence = round.core.sync_presence(&common.lock().unwrap());
                let acted = round.core.automatic_step();
                settle(&service, &key, &mut round);
                if presence || acted {
                    snapshot(&service, &key, &round, &mut dispatch);
                }
            }
            drop(service);
            {
                let senders = senders.lock().await;
                for delivery in dispatch.messages {
                    if let Some(sender) = senders.get(&delivery.recipient)
                        && let Ok(frame) = ws_common::to_text_message(&delivery.payload)
                    {
                        let _ = sender.send(frame);
                    }
                }
            }
            if stop {
                break;
            }
        }
        let mut states = states.lock().unwrap();
        if states.get(&key).is_some_and(|r| Arc::ptr_eq(r, &round)) {
            states.remove(&key);
        }
    });
}

#[cfg(test)]
#[path = "game/tests.rs"]
mod tests;
