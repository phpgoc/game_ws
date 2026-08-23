use share_type_public::{
    DominoesActionSource, DominoesNoPlayableTiles, DominoesPhase, WsDominoesHandState,
    WsDominoesLegalPlay, WsDominoesTableSnapshotEvent,
};

use crate::core::{CoreError, DominoesRoundState, Placement, RoundResult, Tile};

#[derive(Debug, Clone)]
pub(crate) enum ActionEvent {
    Play {
        position: usize,
        placement: Placement,
        score: i32,
        total_score: i32,
        source: DominoesActionSource,
    },
    Draw {
        position: usize,
        boneyard_count: usize,
        tile: Option<Tile>,
        playable: bool,
        source: DominoesActionSource,
    },
    Pass {
        position: usize,
        consecutive_passes: usize,
        source: DominoesActionSource,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ActionOutcome {
    pub events: Vec<ActionEvent>,
    pub snapshot: WsDominoesTableSnapshotEvent,
    pub hands: Vec<(usize, WsDominoesHandState)>,
    pub round_result: Option<RoundResult>,
}

impl ActionOutcome {
    fn finish(
        state: &DominoesRoundState,
        events: Vec<ActionEvent>,
        round_result: Option<RoundResult>,
    ) -> Self {
        Self {
            events,
            snapshot: state.table_snapshot(),
            hands: state
                .positions
                .iter()
                .map(|position| (*position, state.hand_state(*position)))
                .collect(),
            round_result,
        }
    }
}

pub(crate) fn play(
    state: &mut DominoesRoundState,
    position: usize,
    tile_id: i32,
    endpoint_id: Option<i32>,
    source: DominoesActionSource,
) -> Result<ActionOutcome, CoreError> {
    let (placement, score, round_result) = state.play_tile(position, tile_id, endpoint_id)?;
    let total_score = state.scores.get(&position).copied().unwrap_or_default();
    Ok(ActionOutcome::finish(
        state,
        vec![ActionEvent::Play {
            position,
            placement,
            score,
            total_score,
            source,
        }],
        round_result,
    ))
}

pub(crate) fn draw(
    state: &mut DominoesRoundState,
    position: usize,
    source: DominoesActionSource,
) -> Result<ActionOutcome, CoreError> {
    let result = state.draw_tile(position)?;
    let mut events = Vec::new();
    if result.tile.is_some() {
        events.push(ActionEvent::Draw {
            position,
            boneyard_count: state.boneyard.len(),
            tile: result.tile,
            playable: result.playable,
            source,
        });
    }
    if result.passed {
        events.push(ActionEvent::Pass {
            position,
            consecutive_passes: state.consecutive_passes,
            source,
        });
    }
    Ok(ActionOutcome::finish(state, events, result.round_result))
}

pub(crate) fn pass(
    state: &mut DominoesRoundState,
    position: usize,
    source: DominoesActionSource,
) -> Result<ActionOutcome, CoreError> {
    let round_result = state.pass(position)?;
    Ok(ActionOutcome::finish(
        state,
        vec![ActionEvent::Pass {
            position,
            consecutive_passes: state.consecutive_passes,
            source,
        }],
        round_result,
    ))
}

/// 自动完成当前玩家由“无牌可出”规则强制触发的摸牌或过牌。
///
/// 一旦摸到可出的牌便停下，把选择权交还给玩家或 AI；不会代替玩家出牌。
pub(crate) fn forced_no_playable_turn(
    state: &mut DominoesRoundState,
) -> Result<Option<ActionOutcome>, CoreError> {
    if state.phase != DominoesPhase::Play {
        return Ok(None);
    }
    let position = state.current_position;
    if !state.legal_plays(position).is_empty() {
        return Ok(None);
    }

    let mut events = Vec::new();
    let round_result = match state.no_playable_tiles {
        DominoesNoPlayableTiles::PassWithoutDraw => {
            let round_result = state.pass(position)?;
            events.push(ActionEvent::Pass {
                position,
                consecutive_passes: state.consecutive_passes,
                source: DominoesActionSource::Forced,
            });
            round_result
        }
        DominoesNoPlayableTiles::DrawOne | DominoesNoPlayableTiles::KeepDrawing => loop {
            let result = state.draw_tile(position)?;
            if result.tile.is_some() {
                events.push(ActionEvent::Draw {
                    position,
                    boneyard_count: state.boneyard.len(),
                    tile: result.tile,
                    playable: result.playable,
                    source: DominoesActionSource::Forced,
                });
            }
            if result.passed {
                events.push(ActionEvent::Pass {
                    position,
                    consecutive_passes: state.consecutive_passes,
                    source: DominoesActionSource::Forced,
                });
            }
            if result.round_result.is_some() || state.current_position != position {
                break result.round_result;
            }
            if result.playable {
                state.refresh_current_turn();
                break None;
            }
            if state.no_playable_tiles == DominoesNoPlayableTiles::DrawOne {
                break None;
            }
        },
    };

    Ok(Some(ActionOutcome::finish(state, events, round_result)))
}

#[cfg(test)]
#[path = "action/tests.rs"]
mod tests;

pub(crate) fn automatic_turn<F>(
    state: &mut DominoesRoundState,
    source: DominoesActionSource,
    mut choose_play: F,
) -> Result<ActionOutcome, CoreError>
where
    F: FnMut(&DominoesRoundState, usize, &[WsDominoesLegalPlay]) -> WsDominoesLegalPlay,
{
    let position = state.current_position;
    let mut events = Vec::new();
    let round_result = loop {
        let legal_plays = state.legal_plays(position);
        if !legal_plays.is_empty() {
            let selected = choose_play(state, position, &legal_plays);
            let selected = legal_plays
                .iter()
                .find(|play| **play == selected)
                .copied()
                .unwrap_or(legal_plays[0]);
            let (placement, score, result) =
                state.play_tile(position, selected.tile_id, selected.endpoint_id)?;
            let total_score = state.scores.get(&position).copied().unwrap_or_default();
            events.push(ActionEvent::Play {
                position,
                placement,
                score,
                total_score,
                source,
            });
            break result;
        }

        let hand = state.hand_state(position);
        if hand.can_draw {
            let result = state.draw_tile(position)?;
            if result.tile.is_some() {
                events.push(ActionEvent::Draw {
                    position,
                    boneyard_count: state.boneyard.len(),
                    tile: result.tile,
                    playable: result.playable,
                    source,
                });
            }
            if result.passed {
                events.push(ActionEvent::Pass {
                    position,
                    consecutive_passes: state.consecutive_passes,
                    source,
                });
            }
            if result.round_result.is_some() || state.current_position != position {
                break result.round_result;
            }
            continue;
        }

        if hand.can_pass {
            let round_result = state.pass(position)?;
            events.push(ActionEvent::Pass {
                position,
                consecutive_passes: state.consecutive_passes,
                source,
            });
            break round_result;
        }

        return Err(CoreError::WrongPhase);
    };
    Ok(ActionOutcome::finish(state, events, round_result))
}
