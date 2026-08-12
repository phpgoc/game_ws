use std::fmt;

/// 本局达到升级结果的一方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreSide {
    Attacking,
    Defending,
}

/// 一局分数对应的胜方和升级级数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreOutcome {
    pub side: ScoreSide,
    pub levels: u8,
}

impl ScoreOutcome {
    pub const fn attacking(levels: u8) -> Self {
        Self {
            side: ScoreSide::Attacking,
            levels,
        }
    }

    pub const fn defending(levels: u8) -> Self {
        Self {
            side: ScoreSide::Defending,
            levels,
        }
    }
}

/// 标准四人升级玩法中，按上一局结果确定下一局庄家。
///
/// 庄家方过庄时由庄家的对家接庄；闲家上台时由庄家的下家接庄。
pub const fn next_four_player_dealer(current_dealer: usize, side: ScoreSide) -> usize {
    match side {
        ScoreSide::Defending => (current_dealer + 2) % 4,
        ScoreSide::Attacking => (current_dealer + 1) % 4,
    }
}

/// 可配置的标准分数进阶表。
///
/// `attacking_win_score` 是闲家翻庄的分数线，`score_per_level` 是超出或
/// 未达到分数线后每多一档对应的分数，`shutout_bonus_levels` 是闲家 0 分
/// 时额外增加的级数。具体游戏只负责把房间配置转换成这个结构。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreProgression {
    pub attacking_win_score: u32,
    pub score_per_level: u32,
    pub shutout_bonus_levels: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreLevelError {
    NonPositiveWinScore,
    NonPositiveScorePerLevel,
}

impl fmt::Display for ScoreLevelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositiveWinScore => {
                formatter.write_str("attacking win score must be positive")
            }
            Self::NonPositiveScorePerLevel => {
                formatter.write_str("score per level must be positive")
            }
        }
    }
}

impl std::error::Error for ScoreLevelError {}

impl ScoreProgression {
    pub fn new(
        attacking_win_score: u32,
        score_per_level: u32,
        shutout_bonus_levels: u8,
    ) -> Result<Self, ScoreLevelError> {
        if attacking_win_score == 0 {
            return Err(ScoreLevelError::NonPositiveWinScore);
        }
        if score_per_level == 0 {
            return Err(ScoreLevelError::NonPositiveScorePerLevel);
        }
        Ok(Self {
            attacking_win_score,
            score_per_level,
            shutout_bonus_levels,
        })
    }

    /// 根据闲家本局获得的分数计算标准升级结果。
    pub fn outcome(self, attacking_score: i32) -> ScoreOutcome {
        let score = attacking_score.max(0) as u32;
        if score >= self.attacking_win_score {
            let levels = 1 + (score - self.attacking_win_score) / self.score_per_level;
            return ScoreOutcome::attacking(levels.min(u32::from(u8::MAX)) as u8);
        }

        let missing = self.attacking_win_score - score;
        let mut levels = 1 + (missing.saturating_sub(1) / self.score_per_level);
        if score == 0 {
            levels += u32::from(self.shutout_bonus_levels);
        }
        ScoreOutcome::defending(levels.min(u32::from(u8::MAX)) as u8)
    }
}
