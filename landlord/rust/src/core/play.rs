use std::collections::HashMap;

use share_type_public::{LandlordPhase, games::landlord::LANDLORD_CARDS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Combo {
    pub kind: ComboKind,
    pub main_rank: u8,
    pub sequence_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComboKind {
    Rocket,
    Bomb,
    Single,
    Pair,
    Triple,
    TripleSingle,
    TriplePair,
    Straight,
    StraightPairs,
    Plane,
    PlaneWithSingles,
    PlaneWithPairs,
    FourWithTwoSingles,
    FourWithTwoPairs,
}

#[derive(Clone, Copy)]
pub struct PlayValidationContext<'a> {
    pub phase: LandlordPhase,
    pub current_position: usize,
    pub hand: Option<&'a [i32]>,
    pub last_play_position: usize,
    pub last_play: &'a [i32],
}

pub fn can_beat(curr: &Combo, prev: &Combo) -> bool {
    if curr.kind == ComboKind::Rocket {
        return prev.kind != ComboKind::Rocket;
    }
    if curr.kind == ComboKind::Bomb {
        return match prev.kind {
            ComboKind::Rocket => false,
            ComboKind::Bomb => curr.main_rank > prev.main_rank,
            _ => true,
        };
    }
    if prev.kind == ComboKind::Rocket || prev.kind == ComboKind::Bomb {
        return false;
    }
    curr.kind == prev.kind
        && curr.sequence_len == prev.sequence_len
        && curr.main_rank > prev.main_rank
}

pub fn card_rank(card: i32) -> u8 {
    match card {
        53 => 16,
        54 => 17,
        _ => (((card - 1) % 13) + 3) as u8,
    }
}

pub fn cards_in_hand(played: &[i32], hand: &[i32]) -> bool {
    let mut hand_count: HashMap<i32, usize> = HashMap::new();
    for &card in hand {
        *hand_count.entry(card).or_insert(0) += 1;
    }
    for &card in played {
        let Some(cnt) = hand_count.get_mut(&card) else {
            return false;
        };
        if *cnt == 0 {
            return false;
        }
        *cnt -= 1;
    }
    true
}

pub fn classify(cards: &[i32]) -> Option<Combo> {
    if cards.is_empty() || !cards.iter().all(is_valid_card_id) {
        return None;
    }
    let len = cards.len();
    let counts = rank_counts(cards);
    let mut groups: Vec<(u8, usize)> = counts.iter().map(|(&r, &c)| (r, c)).collect();
    groups.sort_by_key(|(r, _)| *r);

    if len == 2 && counts.get(&16) == Some(&1) && counts.get(&17) == Some(&1) {
        return Some(Combo {
            kind: ComboKind::Rocket,
            main_rank: 17,
            sequence_len: 1,
        });
    }
    if len == 4 && groups.len() == 1 && groups[0].1 == 4 {
        return Some(Combo {
            kind: ComboKind::Bomb,
            main_rank: groups[0].0,
            sequence_len: 1,
        });
    }
    if len == 1 {
        return Some(Combo {
            kind: ComboKind::Single,
            main_rank: groups[0].0,
            sequence_len: 1,
        });
    }
    if len == 2 && groups.len() == 1 && groups[0].1 == 2 {
        return Some(Combo {
            kind: ComboKind::Pair,
            main_rank: groups[0].0,
            sequence_len: 1,
        });
    }
    if len == 3 && groups.len() == 1 && groups[0].1 == 3 {
        return Some(Combo {
            kind: ComboKind::Triple,
            main_rank: groups[0].0,
            sequence_len: 1,
        });
    }
    if len == 4 && groups.len() == 2 {
        let triple = groups.iter().find(|(_, c)| *c == 3)?;
        return Some(Combo {
            kind: ComboKind::TripleSingle,
            main_rank: triple.0,
            sequence_len: 1,
        });
    }
    if len == 5 && groups.len() == 2 {
        let triple = groups.iter().find(|(_, c)| *c == 3)?;
        if groups.iter().any(|(_, c)| *c == 2) {
            return Some(Combo {
                kind: ComboKind::TriplePair,
                main_rank: triple.0,
                sequence_len: 1,
            });
        }
    }

    let straight_ranks: Vec<u8> = groups
        .iter()
        .filter_map(|(r, c)| if *c == 1 { Some(*r) } else { None })
        .collect();
    if len >= 5
        && straight_ranks.len() == len
        && straight_ranks.iter().all(|r| *r < 15)
        && is_consecutive(&straight_ranks)
    {
        return Some(Combo {
            kind: ComboKind::Straight,
            main_rank: *straight_ranks.last()?,
            sequence_len: len,
        });
    }

    let pair_ranks: Vec<u8> = groups
        .iter()
        .filter_map(|(r, c)| if *c == 2 { Some(*r) } else { None })
        .collect();
    if len >= 6
        && len % 2 == 0
        && pair_ranks.len() * 2 == len
        && pair_ranks.iter().all(|r| *r < 15)
        && is_consecutive(&pair_ranks)
    {
        return Some(Combo {
            kind: ComboKind::StraightPairs,
            main_rank: *pair_ranks.last()?,
            sequence_len: pair_ranks.len(),
        });
    }

    let triple_ranks: Vec<u8> = groups
        .iter()
        .filter_map(|(r, c)| if *c == 3 { Some(*r) } else { None })
        .collect();
    if triple_ranks.len() >= 2
        && triple_ranks.iter().all(|r| *r < 15)
        && is_consecutive(&triple_ranks)
    {
        let n = triple_ranks.len();
        if len == n * 3 {
            return Some(Combo {
                kind: ComboKind::Plane,
                main_rank: *triple_ranks.last()?,
                sequence_len: n,
            });
        }
        if len == n * 4 {
            let wings = groups
                .iter()
                .filter(|(r, c)| *c == 1 && !triple_ranks.contains(r))
                .count();
            if wings == n {
                return Some(Combo {
                    kind: ComboKind::PlaneWithSingles,
                    main_rank: *triple_ranks.last()?,
                    sequence_len: n,
                });
            }
        }
        if len == n * 5 {
            let wing_pairs = groups
                .iter()
                .filter(|(r, c)| *c == 2 && !triple_ranks.contains(r))
                .count();
            if wing_pairs == n {
                return Some(Combo {
                    kind: ComboKind::PlaneWithPairs,
                    main_rank: *triple_ranks.last()?,
                    sequence_len: n,
                });
            }
        }
    }

    if len == 6 {
        if let Some((rank, _)) = groups.iter().find(|(_, c)| *c == 4) {
            return Some(Combo {
                kind: ComboKind::FourWithTwoSingles,
                main_rank: *rank,
                sequence_len: 1,
            });
        }
    }
    if len == 8 {
        if let Some((rank, _)) = groups.iter().find(|(_, c)| *c == 4) {
            let pair_cnt = groups.iter().filter(|(_, c)| *c == 2).count();
            if pair_cnt == 2 {
                return Some(Combo {
                    kind: ComboKind::FourWithTwoPairs,
                    main_rank: *rank,
                    sequence_len: 1,
                });
            }
        }
    }

    None
}

pub fn validate_play(context: PlayValidationContext<'_>, position: usize, cards: &[i32]) -> bool {
    if context.phase != LandlordPhase::Play || context.current_position != position {
        return false;
    }

    let Some(hand) = context.hand else {
        return false;
    };
    if !cards.iter().all(is_valid_card_id) || !cards_in_hand(cards, hand) {
        return false;
    }

    if cards.is_empty() {
        if context.last_play.is_empty() {
            return false;
        }
        return context.last_play_position != position;
    }

    if context.last_play.is_empty() || context.last_play_position == position {
        return classify(cards).is_some();
    }

    let Some(prev) = classify(context.last_play) else {
        return false;
    };
    let Some(curr) = classify(cards) else {
        return false;
    };
    can_beat(&curr, &prev)
}

fn is_consecutive(ranks: &[u8]) -> bool {
    if ranks.is_empty() {
        return false;
    }
    for i in 1..ranks.len() {
        if ranks[i] != ranks[i - 1] + 1 {
            return false;
        }
    }
    true
}

fn is_valid_card_id(card: &i32) -> bool {
    LANDLORD_CARDS.binary_search(card).is_ok()
}

fn rank_counts(cards: &[i32]) -> HashMap<u8, usize> {
    let mut m: HashMap<u8, usize> = HashMap::new();
    for &c in cards {
        *m.entry(card_rank(c)).or_insert(0) += 1;
    }
    m
}
