//! 可连接端与计分端分开建模。双牌只连接 0/1 张牌时计两份点数，
//! 连接至少两张牌后成为内部节点，剩余可连接方向本身不贡献分数。
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardEnd {
    pub pip: i32,
    pub spinner: bool,
}

/// 双六中每种点数只有一张双牌，因此按点数保存它尚未连接的端口数即可。
/// 此结构只使用已落桌的公开牌，不含任何其他玩家私牌。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardEnds {
    ordinary: [u8; 7],
    spinners: [u8; 7],
}

impl BoardEnds {
    pub fn pips(self) -> Vec<i32> {
        (0..7)
            .flat_map(|pip| {
                std::iter::repeat_n(
                    pip as i32,
                    usize::from(self.ordinary[pip] + self.spinners[pip]),
                )
            })
            .collect()
    }

    pub fn attachments(self) -> impl Iterator<Item = BoardEnd> {
        (0..7).flat_map(move |pip| {
            [false, true].into_iter().filter_map(move |spinner| {
                let count = if spinner {
                    self.spinners[pip]
                } else {
                    self.ordinary[pip]
                };
                (count > 0).then_some(BoardEnd {
                    pip: pip as i32,
                    spinner,
                })
            })
        })
    }

    pub fn after_play(
        mut self,
        rule: DominoesRule,
        tile: Tile,
        attached: Option<BoardEnd>,
    ) -> Self {
        if let Some(end) = attached {
            let counts = if end.spinner {
                &mut self.spinners
            } else {
                &mut self.ordinary
            };
            counts[end.pip as usize] -= 1;
        } else {
            self = Self::default();
        }
        if rule == DominoesRule::FiveUp && tile.is_double() {
            self.spinners[tile.a as usize] = if attached.is_none() { 4 } else { 3 };
        } else if let Some(end) = attached {
            self.ordinary[tile.other_pip(end.pip) as usize] += 1;
        } else {
            self.ordinary[tile.a as usize] += 1;
            self.ordinary[tile.b as usize] += 1;
        }
        self
    }

    pub fn five_up_score(self, rule: DominoesRule) -> i32 {
        if rule != DominoesRule::FiveUp {
            return 0;
        }
        let total = (0..7)
            .map(|pip| {
                pip as i32
                    * (i32::from(self.ordinary[pip]) + if self.spinners[pip] >= 3 { 2 } else { 0 })
            })
            .sum::<i32>();
        if total % 5 == 0 { total } else { 0 }
    }

    #[cfg(test)]
    pub fn ordinary(pips: &[i32]) -> Self {
        let mut board = Self::default();
        for pip in pips {
            board.ordinary[*pip as usize] += 1;
        }
        board
    }
}

impl DominoesRoundState {
    pub fn board_end(&self, endpoint_id: i32) -> Option<BoardEnd> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|end| end.endpoint_id == endpoint_id)?;
        Some(BoardEnd {
            pip: endpoint.pip,
            spinner: self.rule == DominoesRule::FiveUp
                && self.placements.iter().any(|placement| {
                    placement.placement_id == endpoint.placement_id && placement.tile.is_double()
                }),
        })
    }

    pub fn board_ends(&self) -> BoardEnds {
        let mut board = BoardEnds::default();
        for endpoint in &self.endpoints {
            let end = self
                .board_end(endpoint.endpoint_id)
                .expect("existing endpoint");
            let counts = if end.spinner {
                &mut board.spinners
            } else {
                &mut board.ordinary
            };
            counts[end.pip as usize] += 1;
        }
        board
    }
}
