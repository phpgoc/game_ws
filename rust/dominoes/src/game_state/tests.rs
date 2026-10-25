use super::*;
use share_type_public::{DominoesNoPlayableTiles, DominoesRule};

fn state(phase: DominoesPhase) -> DominoesGameState {
    let mut common = CommonGameState::new();
    for position in 0..3 {
        common.add_player(position, position as u64 + 1, &format!("P{position}"));
    }
    let mut round = DominoesRoundState::new_with_seed(
        Arc::new(Mutex::new(common)),
        DominoesRule::FiveUp,
        DominoesNoPlayableTiles::KeepDrawing,
        61,
        11,
    )
    .unwrap();
    if phase == DominoesPhase::StarterSelection {
        round.start_new_game_pending().unwrap();
    } else {
        round.start_new_game().unwrap();
        round.phase = phase;
    }
    DominoesGameState::new(round)
}

#[test]
fn returning_current_player_gets_a_full_manual_window_and_invalidates_the_old_tick() {
    for phase in [DominoesPhase::StarterSelection, DominoesPhase::Play] {
        for reconnect in [false, true] {
            let mut game = state(phase);
            let (position, revision, before) = {
                let mut round = game.inner.lock().unwrap();
                let position = round.current_position;
                round.cap_remaining_seconds(1);
                let mut common = round.base.lock().unwrap();
                if reconnect {
                    common.mark_disconnected(position);
                } else {
                    common.mark_away(position);
                    common.mark_ai_takeover_position(position);
                }
                drop(common);
                (position, round.turn_revision, round.table_snapshot())
            };
            let hand = game.inner.lock().unwrap().hand_state(position);
            if reconnect {
                game.add_player(position, 99, &format!("P{position}"));
            } else {
                game.clear_away_position(position);
            }
            let mut round = game.inner.lock().unwrap();
            assert_eq!(
                round.remaining_seconds, 30,
                "{phase:?}, reconnect={reconnect}"
            );
            assert_ne!(round.turn_revision, revision);
            assert!(!round.tick_remaining_seconds(revision));
            assert!(!round.tick_starter_selection(revision));
            assert_eq!(round.remaining_seconds, 30);
            let after = round.table_snapshot();
            assert_eq!(after.current_position, before.current_position);
            assert_eq!(after.round, before.round);
            assert_eq!(after.placements, before.placements);
            assert_eq!(
                after.starter_selection.as_ref().map(|s| &s.slots),
                before.starter_selection.as_ref().map(|s| &s.slots)
            );
            assert_eq!(round.hand_state(position), hand);
            let common = round.base.lock().unwrap();
            assert!(!common.is_disconnected(position));
            assert!(!common.is_away(position));
            assert!(!common.is_ai_takeover_position(position));
        }
    }
}

#[test]
fn returning_another_seat_or_repeating_manual_presence_keeps_the_current_deadline() {
    for phase in [DominoesPhase::StarterSelection, DominoesPhase::Play] {
        for reconnect in [false, true] {
            for returning_current in [false, true] {
                let mut game = state(phase);
                let (position, revision) = {
                    let mut round = game.inner.lock().unwrap();
                    round.cap_remaining_seconds(1);
                    let position = if returning_current {
                        round.current_position
                    } else {
                        (round.current_position + 1) % 3
                    };
                    if !returning_current {
                        let mut common = round.base.lock().unwrap();
                        common.mark_away(position);
                        common.mark_disconnected(position);
                        common.mark_ai_takeover_position(position);
                    }
                    (position, round.turn_revision)
                };
                if reconnect {
                    game.add_player(position, 99, &format!("P{position}"));
                } else {
                    game.clear_away_position(position);
                }
                let round = game.inner.lock().unwrap();
                assert_eq!(round.remaining_seconds, 1);
                assert_eq!(round.turn_revision, revision);
            }
        }
    }
}

#[test]
fn presence_return_keeps_settlement_and_game_over_countdowns() {
    for phase in [DominoesPhase::RoundOver, DominoesPhase::GameOver] {
        for reconnect in [false, true] {
            let mut game = state(phase);
            let (position, revision) = {
                let mut round = game.inner.lock().unwrap();
                round.remaining_seconds = if phase == DominoesPhase::GameOver {
                    0
                } else {
                    4
                };
                let position = round.current_position;
                let mut common = round.base.lock().unwrap();
                common.mark_away(position);
                common.mark_disconnected(position);
                common.mark_ai_takeover_position(position);
                drop(common);
                (position, round.turn_revision)
            };
            if reconnect {
                game.add_player(position, 99, &format!("P{position}"));
            } else {
                game.clear_away_position(position);
            }
            let round = game.inner.lock().unwrap();
            assert_eq!(round.phase, phase);
            assert_eq!(round.turn_revision, revision);
            assert_eq!(
                round.remaining_seconds,
                if phase == DominoesPhase::GameOver {
                    0
                } else {
                    4
                }
            );
        }
    }
}
