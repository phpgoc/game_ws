use super::*;

fn attach(state: &mut DominoesRoundState, played: Tile, parent: Tile, pip: i32) -> i32 {
    let parent = state
        .placements
        .iter()
        .find(|placement| placement.tile == parent)
        .expect("parent on table")
        .placement_id;
    let endpoint = state
        .endpoints
        .iter()
        .find(|end| end.placement_id == parent && end.pip == pip)
        .expect("matching open end")
        .endpoint_id;
    let predicted = state
        .legal_plays(state.current_position)
        .into_iter()
        .find(|play| play.tile_id == played.id && play.endpoint_id == Some(endpoint))
        .expect("recorded play is legal");
    let (_, score, _) = state
        .play_tile(state.current_position, played.id, Some(endpoint))
        .expect("recorded play");
    assert_eq!(predicted.score, score);
    score
}

fn recorded_endgame() -> DominoesRoundState {
    let mut state = state();
    state.rule = DominoesRule::FiveUp;
    state.no_playable_tiles = DominoesNoPlayableTiles::KeepDrawing;
    state.phase = DominoesPhase::Play;
    state.hands.insert(0, Tile::all());
    // 只重建 03:51 的连接图；以下构造顺序不是录像前半轮的原始行动顺序。
    state.play_tile(0, tile(6, 6).id, None).unwrap();
    for branch in [
        &[(6, 2), (2, 4)][..],
        &[(6, 1), (1, 4), (4, 6)][..],
        &[(6, 0), (0, 4)][..],
        &[(6, 3), (3, 1), (1, 1), (1, 5), (5, 2), (2, 0), (0, 5)][..],
    ] {
        let mut parent = tile(6, 6);
        for &(inward, outward) in branch {
            state.current_position = 0;
            let played = tile(inward, outward);
            attach(&mut state, played, parent, inward);
            parent = played;
        }
    }
    state.current_position = 0;
    state.hands = HashMap::from([
        (0, vec![tile(2, 2), tile(3, 3), tile(5, 6)]),
        (1, vec![tile(0, 3), tile(1, 2)]),
        (2, vec![tile(5, 5)]),
        (3, vec![tile(3, 5)]),
    ]);
    state.boneyard = Tile::all()
        .into_iter()
        .filter(|tile| {
            !state.placements.iter().any(|placed| placed.tile == *tile)
                && !state.hands.values().any(|hand| hand.contains(tile))
                && ![self::tile(4, 4), self::tile(4, 5)].contains(tile)
        })
        .collect();
    // 录像确认的最后两次摸牌；其余四张未摸暗牌只保留集合，不推断顺序。
    state.boneyard.extend([tile(4, 5), tile(4, 4)]);
    state.scores = HashMap::from([(0, 15), (1, 5), (2, 25), (3, 0)]);
    state.round_score_changes = state.scores.clone();
    state
}

fn assert_complete_set(state: &DominoesRoundState) {
    let mut ids = state
        .placements
        .iter()
        .map(|placed| placed.tile.id)
        .chain(state.hands.values().flatten().map(|tile| tile.id))
        .chain(state.boneyard.iter().map(|tile| tile.id))
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids, (0..28).collect::<Vec<_>>());
}

#[test]
fn switch_recording_replays_draws_scores_and_a_change_of_match_leader() {
    // https://www.nicovideo.jp/watch/sm37356452 03:51～04:06。
    // 03:58 左家摸牌，03:59 出 4-4；04:00 中家出 4-5 得 25；
    // 04:02 右家用 3-5 出完并公开余牌，04:04 得到 26 / 5 = 5 分。
    let mut state = recorded_endgame();
    assert_complete_set(&state);
    assert_eq!(state.placements.len(), 15);
    assert_eq!(attach(&mut state, tile(5, 6), tile(0, 5), 5), 20);
    assert_eq!(state.scores[&0], 35);
    assert_eq!(state.phase, DominoesPhase::Play);

    // 已有 6-6 分叉之后，内部 1-1 不能再提供两个额外的 1 端。
    assert!(state.legal_plays(1).is_empty());
    let draw = state
        .draw_tile(1)
        .expect("left CPU draws despite holding 1-2");
    assert_eq!(draw.tile, Some(tile(4, 4)));
    assert!(draw.playable && !draw.passed);
    assert_eq!(attach(&mut state, tile(4, 4), tile(2, 4), 4), 0);

    assert!(state.legal_plays(2).is_empty());
    let draw = state.draw_tile(2).expect("middle CPU draws");
    assert_eq!(draw.tile, Some(tile(4, 5)));
    assert!(draw.playable && !draw.passed);
    assert_eq!(attach(&mut state, tile(4, 5), tile(0, 4), 4), 25);
    assert_eq!(state.scores[&2], 50);
    assert_eq!(state.phase, DominoesPhase::Play);

    assert_eq!(attach(&mut state, tile(3, 5), tile(4, 5), 5), 0);
    assert_complete_set(&state);
    let result = state
        .last_round_result
        .as_ref()
        .expect("last tile ends round");
    assert_eq!(result.winner_position, 3);
    assert_eq!(result.round_score, 5);
    assert_eq!(
        result.scores,
        HashMap::from([(0, 35), (1, 5), (2, 50), (3, 5)])
    );
    assert!(result.game_over);
    assert_eq!(result.winner_positions, vec![2]);
    assert_eq!(state.phase, DominoesPhase::GameOver);
}

#[test]
fn switch_screenshot_forecast_is_thirty_from_two_terminal_doubles_and_two_plain_ends() {
    // https://amesss.hatenablog.com/entry/2024/01/02/120000
    // 20240102004653.jpg 的完整 18 张桌面连接图；构造顺序不冒充原作行动顺序。
    // 中央 5-5 是唯一 spinner，4-4、6-6 都是后来接入的端部双牌。
    let mut state = state();
    state.rule = DominoesRule::FiveUp;
    state.phase = DominoesPhase::Play;
    state.hands.insert(0, Tile::all());
    state.play_tile(0, tile(5, 5).id, None).unwrap();
    for branch in [
        &[(5, 2), (2, 0), (0, 3), (3, 1)][..],
        &[(5, 6), (6, 4), (4, 4)][..],
        &[
            (5, 1),
            (1, 0),
            (0, 5),
            (5, 3),
            (3, 4),
            (4, 2),
            (2, 3),
            (3, 6),
            (6, 6),
        ][..],
        &[(5, 4)][..],
    ] {
        let mut parent = tile(5, 5);
        for &(inward, outward) in branch {
            state.current_position = 0;
            let played = tile(inward, outward);
            attach(&mut state, played, parent, inward);
            parent = played;
        }
    }

    state.current_position = 0;
    state
        .hands
        .insert(0, vec![tile(2, 2), tile(0, 6), tile(1, 6), tile(2, 6)]);
    // 截图只公开另外三家的 1/2/1 张数及两张余库，不推断具体暗牌和顺序。
    let mut unknown = Tile::all().into_iter().filter(|tile| {
        !state.placements.iter().any(|placed| placed.tile == *tile)
            && !state.hands[&0].contains(tile)
    });
    let other_hands = [(1, 1), (2, 2), (3, 1)]
        .map(|(position, count)| (position, unknown.by_ref().take(count).collect::<Vec<_>>()));
    let boneyard = unknown.collect();
    state.hands.extend(other_hands);
    state.boneyard = boneyard;
    state.scores = HashMap::from([(0, 40), (1, 40), (2, 45), (3, 36)]);
    state.round_score_changes.clear();
    state.target_score = 61;
    assert_complete_set(&state);
    assert_eq!(state.placements.len(), 18);
    assert_eq!(state.boneyard.len(), 2);
    assert_eq!(state.endpoints.len(), 4);
    assert_eq!(state.board_ends().pips(), vec![1, 4, 4, 6]);
    assert_eq!(state.five_up_score(), 25);

    let choices = state
        .legal_plays(0)
        .into_iter()
        .filter(|play| play.tile_id == tile(1, 6).id)
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 2);
    for choice in &choices {
        let end = state.board_end(choice.endpoint_id.unwrap()).unwrap();
        assert_eq!(choice.score, if end.pip == 1 { 30 } else { 0 });
    }
    let mut other_endpoint = state.clone();
    assert_eq!(attach(&mut other_endpoint, tile(1, 6), tile(6, 6), 6), 0);
    assert_complete_set(&other_endpoint);

    // 原作选中 1-6 接到 1 的预告：30 = 4*2 + 6*2 + 4 + 6。
    assert_eq!(attach(&mut state, tile(1, 6), tile(1, 3), 1), 30);
    assert_eq!(state.scores[&0], 70);
    assert_eq!(state.round_score_changes[&0], 30);
    assert_eq!(state.hands[&0].len(), 3);
    assert_eq!(state.phase, DominoesPhase::Play);
    assert!(state.last_round_result.is_none());
    assert_complete_set(&state);
}
