use super::*;

pub(in crate::ai::decision) fn is_seven_pairs_wait_shape(hand: &[i32]) -> bool {
    if hand.len() != 13 {
        return false;
    }
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand.iter().filter(|tile| is_valid_tile(**tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    let pairs = counts.values().map(|count| count / 2).sum::<usize>();
    let singles = counts.values().filter(|&&count| count % 2 == 1).count();
    pairs == 6 && singles == 1
}

pub(in crate::ai::decision) fn pair_count(hand: &[i32]) -> usize {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand.iter().filter(|tile| is_valid_tile(**tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    counts.values().map(|count| count / 2).sum()
}

pub(in crate::ai::decision) fn remove_n_tiles(hand: &[i32], tile: i32, count: usize) -> Vec<i32> {
    let mut removed = 0usize;
    let mut next = Vec::with_capacity(hand.len().saturating_sub(count));
    for &item in hand {
        if item == tile && removed < count {
            removed += 1;
        } else {
            next.push(item);
        }
    }
    next
}

pub(in crate::ai::decision) fn single_tile(hand: &[i32]) -> Option<i32> {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand.iter().filter(|tile| is_valid_tile(**tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    counts
        .into_iter()
        .find_map(|(tile, count)| (count % 2 == 1).then_some(tile))
}
