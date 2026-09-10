//! 仅在 e2e-fixture 构建中启用的完整合法牌山；客户端仍走正常发牌与动作校验。
//! 不接受客户端手牌/窗口注入，也不改变胡牌规则或计时。

use share_type_public::games::shenyang_mahjong::SHENYANG_MAHJONG_TILE_KINDS;

const DUAL_ROB_GANG_HANDS: [[i32; 13]; 4] = [
    [1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35],
    [1, 2, 7, 8, 9, 14, 15, 16, 24, 25, 26, 36, 36],
    [3, 4, 5, 6, 7, 8, 9, 17, 18, 19, 27, 28, 29],
    [3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31],
];

pub(crate) fn wall_for_fixture(
    name: Option<&str>,
    round_index: u64,
    dealer: usize,
) -> Option<Vec<i32>> {
    if name != Some("dual-rob-gang") || round_index != 0 || dealer != 0 {
        return None;
    }
    let mut draw_order = Vec::with_capacity(136);
    for index in 0..13 {
        for hand in DUAL_ROB_GANG_HANDS {
            draw_order.push(hand[index]);
        }
    }
    // 庄家弃 37，座位 1 摸弃 34，座位 2 摸 32 后弃 3；座位 3 碰 3 后
    // 弃 31，下一圈三家分别摸弃 33/34/32，再由座位 3 摸到第四张 3 加杠。
    draw_order.extend([37, 34, 32, 33, 34, 32, 3]);
    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let used = draw_order.iter().filter(|&&value| value == tile).count();
        assert!(used <= 4, "E2E wall contains a fifth tile");
        draw_order.extend(std::iter::repeat_n(tile, 4 - used));
    }
    assert_eq!(draw_order.len(), 136);
    draw_order.reverse(); // 正常发牌和摸牌均从 Vec 尾部 pop。
    Some(draw_order)
}

#[cfg(test)]
mod tests;
