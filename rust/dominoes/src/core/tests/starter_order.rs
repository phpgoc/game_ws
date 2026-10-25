use super::*;

fn select_tiles(rule: DominoesRule, choices: &[Tile], slots: &[usize]) -> usize {
    let mut state = state_with_players(choices.len());
    state.rule = rule;
    state.start_new_game_pending().expect("pending start");
    let mut pool = vec![None; 6];
    for (&choice, &slot) in choices.iter().zip(slots) {
        pool[slot] = Some(choice);
    }
    let mut unselected = Tile::all()
        .into_iter()
        .filter(|tile| !choices.contains(tile));
    state.starter_pool = pool
        .into_iter()
        .map(|selected| selected.unwrap_or_else(|| unselected.next().expect("unused tile")))
        .collect();
    let mut starter = None;
    for (position, &slot) in slots.iter().enumerate() {
        let result = state.select_starter_slot(position, slot).expect("select");
        if position + 1 < choices.len() {
            assert_eq!(result.starter_position, None);
            assert!(state.hands.is_empty());
        } else {
            starter = result.starter_position;
        }
    }
    let starter = starter.expect("starter after all selections");
    assert_eq!(state.current_position, starter);
    assert_eq!(state.round_starter, starter);
    assert_eq!(state.phase, DominoesPhase::Play);
    assert!(state.hands.values().all(|hand| hand.len() == 5));
    starter
}

#[test]
fn starter_ties_match_switch_recordings() {
    // sm37089085 13:24.55 玩家先选槽 0；13:26.63 CPU 后选槽 1，
    // 13:27.37 的最高点数为 4-4 / 2-6，原作判持 2-6 的 CPU 先手。
    // sm42098821 03:24.215 的最高同点为 5-5 / 4-6，判 4-6 先手。
    for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
        assert_eq!(
            select_tiles(
                rule,
                &[tile(4, 4), tile(2, 6), tile(0, 2), tile(3, 3)],
                &[0, 1, 2, 3]
            ),
            1
        );
        assert_eq!(
            select_tiles(
                rule,
                &[tile(1, 3), tile(4, 6), tile(5, 5), tile(2, 3)],
                &[4, 3, 1, 2]
            ),
            1
        );
    }
}

#[test]
fn starter_ties_do_not_depend_on_seats_or_hidden_slot_order() {
    // 位置变化是项目回归；三人局属于扩展，不是原作实测样本。
    for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
        for player_count in [3, 4] {
            for winner in 0..player_count {
                let mut choices = vec![tile(4, 4), tile(2, 6), tile(0, 2), tile(3, 3)];
                choices.swap(1, winner);
                choices.truncate(player_count);
                for slots in [&[0, 1, 2, 3][..player_count], &[5, 3, 2, 0][..player_count]] {
                    assert_eq!(select_tiles(rule, &choices, slots), winner);
                }
            }
        }
    }
}

#[test]
fn starter_total_pips_take_priority_over_the_larger_end() {
    for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
        assert_eq!(
            select_tiles(rule, &[tile(0, 6), tile(4, 4), tile(0, 2)], &[5, 2, 0]),
            1
        );
    }
}
