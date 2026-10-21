use super::*;
use rand::{SeedableRng, rngs::StdRng};
fn room() -> Room {
    Room::new(
        "room".into(),
        ["A".into(), "B".into(), "C".into()],
        &mut StdRng::seed_from_u64(42),
    )
}
#[test]
fn deal_and_roles_remain_private_and_buried_cards_become_public() {
    for seed in 0..30 {
        let mut room = Room::new(
            "room".into(),
            ["A".into(), "B".into(), "C".into()],
            &mut StdRng::seed_from_u64(seed),
        );
        let landlord = room.landlord_position;
        let deck = room
            .players
            .iter()
            .flat_map(|p| p.hand.iter().copied())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(deck, (1..=54).collect());
        for position in 0..3 {
            let state = room.state_for(position);
            assert_eq!(state.hand.len(), if position == landlord { 20 } else { 17 });
            assert_eq!(
                state.landlord_position,
                (position == landlord).then_some(landlord as i32)
            );
            for player in state.players {
                assert_eq!(
                    player.card_count,
                    if position == landlord && player.position == position as i32 {
                        20
                    } else {
                        17
                    }
                );
                if player.position != position as i32 {
                    assert_eq!(player.role, WhoIsLandlordRole::UNKNOWN);
                }
            }
        }
        let cards = room.players[landlord].hand[..3].to_vec();
        assert!(!room.apply(
            (landlord + 1) % 3,
            WhoIsLandlordAction::Bury {
                cards: cards.clone()
            }
        ));
        assert!(!room.apply(
            landlord,
            WhoIsLandlordAction::Bury {
                cards: vec![cards[0]; 3]
            }
        ));
        assert!(room.apply(landlord, WhoIsLandlordAction::Bury { cards }));
        assert_eq!(room.current_position, room.first_position);
        for position in 0..3 {
            let state = room.state_for(position);
            assert_eq!(state.bottom_cards.len(), 3);
            assert_eq!(state.hand.len(), 17);
            assert!(state.players.iter().all(|p| p.card_count == 17));
        }
    }
}
#[test]
fn full_takeover_finishes_and_settlement_is_zero_sum() {
    let mut room = room();
    for player in &mut room.players {
        player.away = true;
    }
    let mut steps = 0;
    while room.automatic_step() {
        steps += 1;
        assert!(steps <= 216);
    }
    assert_eq!(room.phase, WhoIsLandlordPhase::SETTLEMENT);
    assert_eq!(room.scores.iter().sum::<i32>(), 0);
    assert!(
        room.state_for(0)
            .players
            .iter()
            .all(|p| p.role != WhoIsLandlordRole::UNKNOWN)
    );
}
#[test]
fn bombs_and_rockets_double_scores_for_both_sides() {
    for (cards, winner, scores) in [
        (vec![1, 14, 27, 40], 0, [4, -2, -2]),
        (vec![53, 54], 1, [-4, 2, 2]),
    ] {
        let mut room = room();
        room.phase = WhoIsLandlordPhase::PLAY;
        room.landlord_position = 0;
        room.current_position = winner;
        room.players[winner].hand = cards.clone();
        assert!(room.apply(winner, WhoIsLandlordAction::Play { cards }));
        assert_eq!(room.phase, WhoIsLandlordPhase::SETTLEMENT);
        assert_eq!(room.scores, scores);
    }
}
#[test]
fn pass_twice_returns_lead_and_invalid_turn_does_not_mutate() {
    let mut room = room();
    room.phase = WhoIsLandlordPhase::PLAY;
    room.current_position = 0;
    room.players[0].hand = vec![1, 2];
    assert!(!room.apply(1, WhoIsLandlordAction::Pass));
    assert!(!room.apply(0, WhoIsLandlordAction::Pass));
    assert!(room.apply(0, WhoIsLandlordAction::Play { cards: vec![1] }));
    assert!(room.apply(1, WhoIsLandlordAction::Pass));
    assert!(room.apply(2, WhoIsLandlordAction::Pass));
    assert_eq!(room.current_position, 0);
    assert!(room.last_play.is_none());
}
#[test]
/// 验证 AI 会用更大的顺子压制而不是错误过牌。
fn brain_plays_a_higher_straight_instead_of_passing() {
    let mut round = room();
    let room = &mut round;
    room.phase = WhoIsLandlordPhase::PLAY;
    room.current_position = 1;
    room.players[1].away = true;
    room.players[1].hand = vec![15, 16, 17, 18, 19];
    room.last_play = Some(Play {
        position: 0,
        cards: vec![1, 2, 3, 4, 5],
        combo: classify(&[1, 2, 3, 4, 5]).unwrap(),
    });

    let BrainAction::Play(cards) = brain_action(room, 1) else {
        panic!("brain should answer a beatable straight");
    };
    let combo = classify(&cards).expect("legal brain cards");
    assert_eq!(combo.kind, ComboKind::Straight);
    assert!(can_beat(combo, room.last_play.as_ref().unwrap().combo));
}
