use crate::Rank;

/// 标准升级从 3 到 A 的级牌顺序。
pub const STANDARD_LEVEL_RANKS: [Rank; 12] = [
    Rank::Three,
    Rank::Four,
    Rank::Five,
    Rank::Six,
    Rank::Seven,
    Rank::Eight,
    Rank::Nine,
    Rank::Ten,
    Rank::Jack,
    Rank::Queen,
    Rank::King,
    Rank::Ace,
];

/// 构造不超过终局级牌、且排除指定牌面的实际升级路径。
///
/// 拖拉机可以通过 `excluded` 表达删牌配置；标准升级传空切片即可。
/// 如果配置的终局级牌本身及之前的牌都被排除，仍保留第一张可用级牌，
/// 避免产生无法开始的空对局。
pub fn level_rank_path(final_rank: Rank, excluded: &[Rank]) -> Vec<Rank> {
    let mut path = STANDARD_LEVEL_RANKS
        .into_iter()
        .take_while(|rank| *rank <= final_rank)
        .filter(|rank| !excluded.contains(rank))
        .collect::<Vec<_>>();

    if path.is_empty()
        && let Some(first) = STANDARD_LEVEL_RANKS
            .into_iter()
            .find(|rank| !excluded.contains(rank))
    {
        path.push(first);
    }
    path
}

/// 计算本局结算后的下一张级牌。
///
/// 至少升一级；跨过终局级牌时封顶到终局级牌。当前已经位于路径终点时
/// 返回 `None`，由游戏服务将本场标记为结束。
pub fn next_level_rank(
    current_rank: Rank,
    final_rank: Rank,
    excluded: &[Rank],
    levels: usize,
) -> Option<Rank> {
    let path = level_rank_path(final_rank, excluded);
    let current = path.iter().position(|rank| *rank == current_rank)?;
    if current + 1 >= path.len() {
        return None;
    }
    path.get((current + levels.max(1)).min(path.len() - 1))
        .copied()
}
