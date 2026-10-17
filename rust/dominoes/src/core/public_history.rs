//! 只记录可从公开动作推断的牌局信息，不保存其他玩家手牌或牌库顺序。

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicPlayContext {
    pub rule: DominoesRule,
    pub no_playable_tiles: DominoesNoPlayableTiles,
    pub target_score: i32,
    pub positions: Vec<usize>,
    pub current_position: usize,
    pub hand_sizes: Vec<(usize, Option<usize>)>,
    pub played_tile_ids: Vec<i32>,
    pub void_pips: Vec<(usize, u8)>,
    pub scores: Vec<(usize, i32)>,
    pub boneyard_count: usize,
    pub consecutive_passes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicPlayEvidence {
    pub tile: Tile,
    pub endpoint_id: Option<i32>,
    pub immediate_score: i32,
    pub remaining_tiles: usize,
    pub open_endpoints: Vec<(i32, i32)>,
    pub context: PublicPlayContext,
}

impl DominoesRoundState {
    /// 只约束当前手牌；实际摸牌后撤销，其他玩家落牌不撤销。
    pub fn inferred_missing_pips(&self, position: usize) -> u8 {
        self.public_missing_pips
            .get(&position)
            .copied()
            .unwrap_or_default()
    }

    /// 该玩家最近一次落牌的公开证据；该玩家实际摸牌后失效。
    pub fn last_public_play(&self, position: usize) -> Option<&PublicPlayEvidence> {
        self.public_plays.get(&position).map(Arc::as_ref)
    }

    pub(super) fn record_public_missing_pips(&mut self, position: usize) {
        let mask = self
            .endpoints
            .iter()
            .fold(0_u8, |mask, endpoint| mask | (1_u8 << endpoint.pip));
        *self.public_missing_pips.entry(position).or_default() |= mask;
    }

    pub(super) fn public_play_evidence(
        &self,
        tile: Tile,
        endpoint_id: Option<i32>,
        remaining_tiles: usize,
    ) -> PublicPlayEvidence {
        PublicPlayEvidence {
            tile,
            endpoint_id,
            immediate_score: self.preview_five_up_score(tile, endpoint_id),
            remaining_tiles,
            open_endpoints: self
                .endpoints
                .iter()
                .map(|endpoint| (endpoint.endpoint_id, endpoint.pip))
                .collect(),
            context: PublicPlayContext {
                rule: self.rule,
                no_playable_tiles: self.no_playable_tiles,
                target_score: self.target_score,
                positions: self.positions.clone(),
                current_position: self.current_position,
                hand_sizes: self
                    .positions
                    .iter()
                    .map(|position| (*position, self.hands.get(position).map(Vec::len)))
                    .collect(),
                played_tile_ids: self
                    .placements
                    .iter()
                    .map(|placement| placement.tile.id)
                    .collect(),
                void_pips: self
                    .positions
                    .iter()
                    .map(|position| (*position, self.inferred_missing_pips(*position)))
                    .collect(),
                scores: self
                    .positions
                    .iter()
                    .map(|position| {
                        (
                            *position,
                            self.scores.get(position).copied().unwrap_or_default(),
                        )
                    })
                    .collect(),
                boneyard_count: self.boneyard.len(),
                consecutive_passes: self.consecutive_passes,
            },
        }
    }
}
