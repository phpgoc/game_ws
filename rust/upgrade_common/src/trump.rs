use crate::{Card, Rank, Suit};

const FIRST_LEVEL_RANK: i32 = Rank::Three as i32;
const LAST_LEVEL_RANK: i32 = Rank::Ace as i32;

/// 返回普通花色牌在抽走当前级牌后的连续位置。
///
/// `2`、大小王和当前级牌不属于普通花色序列。级牌被抽走后两侧会补平，
/// 例如打 `5` 时 `4` 与 `6` 的位置相邻。
pub fn compact_plain_rank_position(rank: Rank, target_rank: Rank) -> Option<i32> {
    let rank = rank as i32;
    let target_rank = target_rank as i32;
    if !(FIRST_LEVEL_RANK..=LAST_LEVEL_RANK).contains(&rank) || rank == target_rank {
        return None;
    }

    let removed_before = i32::from(
        (FIRST_LEVEL_RANK..=LAST_LEVEL_RANK).contains(&target_rank) && rank > target_rank,
    );
    Some(rank - FIRST_LEVEL_RANK - removed_before)
}

fn ordinary_trump_count(target_rank: Rank) -> i32 {
    (FIRST_LEVEL_RANK..=LAST_LEVEL_RANK)
        .filter(|rank| *rank != target_rank as i32)
        .count() as i32
}

/// 返回一张主牌在完整主牌连续序列中的位置，弱牌位置较小。
///
/// 标准等级（3–A）下的顺序为：普通主牌、主花色 `2`、`副级`、`主级`、
/// 小王、大王。普通主牌抽走当前级牌后补平，因此最高保留的普通主牌与
/// 主花色 `2` 相邻。当前级为 `2` 时，四种 `2` 都是级牌，顺序为副级、
/// 主级、小王、大王；非主花色的普通 `2` 不进入主牌组。
pub fn trump_order_position(
    card: Card,
    target_rank: Rank,
    trump_suit: Option<Suit>,
) -> Option<i32> {
    let rank = card.rank();
    let suit = card.suit();
    let ordinary_count = ordinary_trump_count(target_rank);
    let joker_offset = if target_rank == Rank::Two { 2 } else { 3 };

    match suit {
        None => match rank {
            Rank::SmallJoker => Some(ordinary_count + joker_offset),
            Rank::BigJoker => Some(ordinary_count + joker_offset + 1),
            _ => None,
        },
        Some(suit) if rank == target_rank => {
            let level_offset = if target_rank == Rank::Two { 0 } else { 1 };
            Some(ordinary_count + level_offset + i32::from(Some(suit) == trump_suit))
        }
        Some(suit) if rank == Rank::Two && Some(suit) == trump_suit => Some(ordinary_count),
        Some(suit) if Some(suit) == trump_suit => compact_plain_rank_position(rank, target_rank),
        Some(_) => None,
    }
}

/// 大小王、当前级牌和主花色牌都属于主牌组；普通 `2` 只有在主花色中，
/// 或当前级本身为 `2` 时，才属于主牌组。
pub fn card_is_trump(card: Card, target_rank: Rank, trump_suit: Option<Suit>) -> bool {
    trump_order_position(card, target_rank, trump_suit).is_some()
}
