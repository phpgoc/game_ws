use super::*;

#[test]
fn ai_declares_dragon_xi_gang_even_from_live_pure_one_suit_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 31, 32, 33, 35, 36, 37];

    assert!(pure_one_suit_plan_score_for_context(&hand, &[], &table, 0,) > 0.0);
    assert_eq!(
        choose_xi_gang_from_view(&hand, &[vec![35, 36, 37]], &table, 0,),
        Some(vec![35, 36, 37])
    );
}

#[test]
fn ai_declares_dragon_xi_gang_that_preserves_four_gui_yi() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 35, 35, 35, 35, 36, 36, 37];

    assert_eq!(
        choose_xi_gang_from_view(&hand, &[vec![35, 36, 37]], &table, 0,),
        Some(vec![35, 36, 37])
    );

    let next_hand = vec![1, 2, 3, 11, 12, 13, 21, 35, 35, 35, 36];
    let xi_gang = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::XI_GANG,
        tiles: vec![35, 36, 37],
        from_position: None,
    };
    assert_eq!(estimated_visible_bonus_fan(&next_hand, &[xi_gang]), 3);
}

#[test]
fn ai_declares_dragon_xi_gang_when_multiple_dragons_are_triplets() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 35, 35, 35, 36, 36, 36, 37];

    assert_eq!(
        choose_xi_gang_from_view(&hand, &[vec![35, 36, 37]], &table, 0,),
        Some(vec![35, 36, 37])
    );
}

#[test]
fn ai_declares_wind_xi_gang_before_dragon_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 31, 32, 33, 34, 35, 36, 37];
    let options = vec![vec![35, 36, 37], vec![31, 32, 33, 34]];

    assert_eq!(
        choose_xi_gang_from_view(&hand, &options, &table, 0),
        Some(vec![31, 32, 33, 34])
    );
}

#[test]
fn ai_declares_wind_xi_gang_even_from_locked_seven_pairs() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 31, 32, 33, 34];

    assert!(should_lock_seven_pairs_plan(&hand, &[], &table, 0,));
    assert_eq!(
        choose_xi_gang_from_view(&hand, &[vec![31, 32, 33, 34]], &table, 0,),
        Some(vec![31, 32, 33, 34])
    );
}

#[test]
fn ai_does_not_declare_wind_xi_gang_without_replacement_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 0;
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_xi_gang_from_view(&hand, &[vec![31, 32, 33, 34]], &table, 0,),
        None
    );
}

#[test]
fn ai_normally_declares_dragon_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 36, 37];

    assert_eq!(
        choose_xi_gang_from_view(&hand, &[vec![37, 35, 36]], &table, 0,),
        Some(vec![35, 36, 37])
    );
}

#[test]
fn ai_preserves_multiple_dragon_pairs_over_dragon_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35, 36, 36, 37];

    assert_eq!(
        choose_xi_gang_from_view(&hand, &[vec![35, 36, 37]], &table, 0,),
        None
    );
}

#[test]
fn double_xi_gang_discard_keeps_only_tile_of_third_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::XI_GANG,
            tiles: vec![31, 32, 33, 34],
            from_position: None,
        },
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::XI_GANG,
            tiles: vec![35, 36, 37],
            from_position: None,
        },
    ];
    let hand = vec![1, 2, 3, 4, 11, 12, 13, 21];

    let discard = choose_discard_from_view(&hand, &table, 0)
        .expect("double xi gang hand should choose a discard");
    assert_ne!(discard, 21);
}

#[test]
fn two_xi_gangs_count_toward_visible_fan_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(3);
    let hand = vec![11, 12, 13, 21, 21];
    let melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::XI_GANG,
            tiles: vec![31, 32, 33, 34],
            from_position: None,
        },
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::XI_GANG,
            tiles: vec![35, 36, 37],
            from_position: None,
        },
        test_chi_meld(1),
    ];

    assert_eq!(estimated_visible_bonus_fan(&hand, &melds), 2);
    assert_eq!(estimated_visible_fan_without_wait(&hand, &melds), 3);
    assert!(capped_open_normal_route_visible_fan_reaches_cap(
        &hand, &melds, &table
    ));
}
