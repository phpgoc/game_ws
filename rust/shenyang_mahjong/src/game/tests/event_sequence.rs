use super::*;
use share_type_public::games::shenyang_mahjong::{
    ShenyangMahjongRoutes, WsShenyangMahjongEventContext,
};

fn cursors(dispatch: &Dispatch, session: SessionId) -> Vec<WsShenyangMahjongEventContext> {
    dispatch
        .messages
        .iter()
        .filter_map(|item| match &item.payload {
            OutboundPayload::Event(event)
                if item.recipient == session && events::is_sequenced_event(event.code) =>
            {
                Some(serde_json::from_value(event.data["event_context"].clone()).unwrap())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn private_snapshots_do_not_create_gaps_for_peers_and_rounds_keep_the_stream() {
    let (room, _, key, shared) = setup_request_room();
    let mut state = shared.lock().unwrap();
    state.deal_new_round();
    let initial_round = state.action_context();
    let mut dispatch = Dispatch::default();
    push_private_deal_events(&room, &key, &state, &default_configs(), &mut dispatch);
    for _ in 0..3 {
        build_table_snapshot_event_with_configs(&state, 1, &default_configs());
    }
    push_private_table_snapshot(&state, &default_configs(), &mut dispatch, 1);
    push_phase_change(
        &room,
        &key,
        &state,
        &mut dispatch,
        state.phase,
        state.current_position,
        3,
    );
    state.deal_new_round();
    push_private_deal_events(&room, &key, &state, &default_configs(), &mut dispatch);
    assert_ne!(state.action_context().round_id, initial_round.round_id);
    for session in 1..=4 {
        let contexts = cursors(&dispatch, session);
        let count = if session == 2 { 4 } else { 3 };
        assert_eq!(contexts.len(), count);
        for (index, cursor) in contexts.iter().enumerate() {
            assert_eq!(cursor.event_seq, index as u32 + 1);
            assert_eq!(cursor.stream_id, contexts[0].stream_id);
        }
    }
    assert_ne!(
        cursors(&dispatch, 1)[0].stream_id,
        cursors(&dispatch, 2)[0].stream_id
    );
}

#[test]
fn shared_claim_identity_has_independently_ordered_claim_and_settlement_events() {
    let (mut room, mut handler, shared) = super::claim_updates::rob_gang_room();
    let context = shared.lock().unwrap().action_context();
    let mut dispatches = Vec::new();
    for session in [2, 3] {
        let mut request = play_request(ShenyangMahjongAction::HU, Vec::new(), Some(6), Some(0));
        request.data["action_context"] = serde_json::to_value(&context).unwrap();
        let dispatch = handler.handle_game_request(&mut room, session, request);
        assert_eq!(
            response_code(&dispatch, session, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        dispatches.push(dispatch);
    }
    let all: Vec<_> = dispatches
        .iter()
        .flat_map(|dispatch| cursors(dispatch, 2))
        .collect();
    for pair in all.windows(2) {
        assert_eq!(pair[0].stream_id, pair[1].stream_id);
        assert_eq!(pair[1].event_seq, pair[0].event_seq + 1);
    }
    assert!(has_room_event(&dispatches[1], WsCode::CHANGE_PHASE));
    assert!(has_room_event(&dispatches[1], WsCode::GAME_OVER));
}

#[test]
fn sync_returns_only_the_callers_private_snapshot_and_echoes_its_request() {
    let (mut room, mut handler, _, shared) = setup_request_room();
    {
        let mut state = shared.lock().unwrap();
        state.deal_new_round();
        state.hands.insert(0, vec![1, 1]);
        state.hands.insert(1, vec![2, 2]);
    }
    let before = shared.lock().unwrap().action_context();
    let dispatch = handler.handle_game_request(
        &mut room,
        2,
        ClientRequest {
            route: ShenyangMahjongRoutes::SYNC_TABLE as i32,
            data: json!({"request_id": "recovery-1"}),
        },
    );
    assert!(dispatch.messages.iter().all(|item| item.recipient == 2));
    let event = dispatch
        .messages
        .iter()
        .find_map(|item| match &item.payload {
            OutboundPayload::Event(event) => Some(event),
            _ => None,
        })
        .unwrap();
    assert_eq!(event.code, WsCode::TABLE_SNAPSHOT as i32);
    assert_eq!(event.data["my_tiles"], json!([2, 2]));
    assert_eq!(event.data["sync_request_id"], "recovery-1");
    assert!(
        event.data["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|player| player.get("hand_tiles").is_none())
    );
    assert_eq!(shared.lock().unwrap().action_context(), before);
    assert_eq!(cursors(&dispatch, 2)[0].event_seq, 1);
    for (session, data) in [
        (99, json!({"request_id":"x"})),
        (2, json!({})),
        (2, json!({"request_id":"x".repeat(65)})),
    ] {
        let rejected = handler.handle_game_request(
            &mut room,
            session,
            ClientRequest {
                route: ShenyangMahjongRoutes::SYNC_TABLE as i32,
                data,
            },
        );
        assert!(!has_room_event(&rejected, WsCode::TABLE_SNAPSHOT));
    }
}

#[test]
fn counter_exhaustion_changes_stream_instead_of_reusing_a_sequence() {
    let state = playable_state();
    let mut dispatch = Dispatch::default();
    push_private_table_snapshot(&state, &default_configs(), &mut dispatch, 0);
    let first = cursors(&dispatch, 1)[0].clone();
    state
        .event_streams
        .lock()
        .unwrap()
        .get_mut(&1)
        .unwrap()
        .event_seq = u32::MAX;
    push_private_table_snapshot(&state, &default_configs(), &mut dispatch, 0);
    let rotated = cursors(&dispatch, 1)[1].clone();
    assert_eq!(rotated.event_seq, 1);
    assert_ne!(rotated.stream_id, first.stream_id);
}
