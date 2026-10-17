use super::claim_updates::rob_gang_room;
use super::*;
use share_type_public::games::shenyang_mahjong::WsShenyangMahjongActionContext;

fn claim(context: &WsShenyangMahjongActionContext, action: ShenyangMahjongAction) -> ClientRequest {
    let mut request = play_request(action, Vec::new(), Some(6), Some(0));
    request.data["action_context"] = serde_json::to_value(context).unwrap();
    request
}

#[test]
fn old_identity_is_rejected_even_when_new_claim_has_identical_legal_shape() {
    for action in [ShenyangMahjongAction::HU, ShenyangMahjongAction::PASS] {
        let (mut room, mut handler, shared) = rob_gang_room();
        let old_context = shared.lock().unwrap().action_context();
        // Deliberately retain the exact same legal hand, source, tile and participants.
        // Shape validation cannot distinguish this later decision window.
        shared.lock().unwrap().advance_action_context();
        let current_context = shared.lock().unwrap().action_context();
        let before = serde_json::to_value(build_table_snapshot_event_with_configs(
            &shared.lock().unwrap(),
            1,
            &default_configs(),
        ))
        .unwrap();
        let rejected = handler.handle_game_request(&mut room, 2, claim(&old_context, action));
        assert_eq!(
            response_code(&rejected, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        assert_eq!(rejected.messages.len(), 1);
        assert_eq!(
            serde_json::to_value(build_table_snapshot_event_with_configs(
                &shared.lock().unwrap(),
                1,
                &default_configs(),
            ))
            .unwrap(),
            before
        );
        let accepted = handler.handle_game_request(&mut room, 2, claim(&current_context, action));
        assert_eq!(
            response_code(&accepted, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
    }
}

#[test]
fn round_identity_rejects_a_request_from_an_identical_recreated_table() {
    let (_, _, old) = rob_gang_room();
    let old_context = old.lock().unwrap().action_context();
    let (mut room, mut handler, current) = rob_gang_room();
    let context = current.lock().unwrap().action_context();
    assert_eq!(old_context.action_id, context.action_id);
    assert_ne!(old_context.round_id, context.round_id);
    let rejected =
        handler.handle_game_request(&mut room, 2, claim(&old_context, ShenyangMahjongAction::HU));
    assert_eq!(
        response_code(&rejected, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    assert!(
        current
            .lock()
            .unwrap()
            .claim_window
            .as_ref()
            .unwrap()
            .responses
            .is_empty()
    );
    let accepted =
        handler.handle_game_request(&mut room, 2, claim(&context, ShenyangMahjongAction::HU));
    assert_eq!(
        response_code(&accepted, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
}

#[test]
fn partial_response_keeps_the_other_players_request_and_snapshot_context_valid() {
    let (mut room, mut handler, shared) = rob_gang_room();
    let context = shared.lock().unwrap().action_context();
    let first =
        handler.handle_game_request(&mut room, 2, claim(&context, ShenyangMahjongAction::HU));
    assert_eq!(
        response_code(&first, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    let state = shared.lock().unwrap();
    assert_eq!(state.action_context(), context);
    let snapshot = build_table_snapshot_event_with_configs(&state, 2, &default_configs());
    assert_eq!(snapshot.action_context, Some(context.clone()));
    assert_eq!(
        snapshot.claim_window.unwrap().action_context,
        Some(context.clone())
    );
    for message in first.messages {
        if let OutboundPayload::Event(event) = message.payload
            && event.code == WsCode::CLAIM_WINDOW as i32
        {
            assert_eq!(
                event.data["action_context"],
                serde_json::to_value(&context).unwrap()
            );
        }
    }
    drop(state);
    let second =
        handler.handle_game_request(&mut room, 3, claim(&context, ShenyangMahjongAction::HU));
    assert_eq!(
        response_code(&second, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    let state = shared.lock().unwrap();
    assert_eq!(
        state.settlement.as_ref().unwrap().winner_positions,
        vec![1, 2]
    );
    assert_ne!(state.action_context(), context);
}

#[test]
fn deal_draw_and_tile_consumption_advance_identity_but_reads_and_failed_actions_do_not() {
    let mut state = playable_state();
    state.deal_new_round();
    let dealt = state.action_context();
    let snapshot = build_table_snapshot_event_with_configs(&state, 0, &default_configs());
    assert_eq!(snapshot.action_context, Some(dealt.clone()));
    assert!(!state.remove_tiles_from_hand(0, &[99]));
    state.set_turn_countdown(2);
    assert_eq!(state.action_context(), dealt);
    let tile = state.hands[&0][0];
    assert!(state.remove_tiles_from_hand(0, &[tile]));
    let discarded = state.action_context();
    assert_eq!(discarded.round_id, dealt.round_id);
    assert!(discarded.action_id > dealt.action_id);
    assert!(state.draw_for_position(0).is_some());
    assert!(state.action_context().action_id > discarded.action_id);
    state.deal_new_round();
    assert_ne!(state.action_context().round_id, dealt.round_id);
}

#[test]
fn context_clients_cannot_omit_identity_after_their_first_contextual_request() {
    let mut state = playable_state();
    let context = state.action_context();
    assert!(state.accepts_action_context(1, None));
    assert!(state.accepts_action_context(1, Some(&context)));
    state.deal_new_round();
    assert!(!state.accepts_action_context(1, None));
    assert!(!state.accepts_action_context(1, Some(&context)));
    assert!(state.accepts_action_context(2, None));
    assert!(state.accepts_action_context(1, Some(&state.action_context())));
}
