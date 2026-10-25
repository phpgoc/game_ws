use rand::seq::SliceRandom;
use share_type_public::games::who_is_landlord::*;
use std::collections::BTreeMap;
const PLAYER_COUNT: usize = 3;
const DECK_SIZE: usize = 54;
const HAND_SIZE: usize = 17;
const BASE_SCORE: i32 = 1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComboKind {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Combo {
    kind: ComboKind,
    main_rank: i32,
    sequence_len: usize,
}

#[derive(Debug, Clone)]
struct Play {
    position: usize,
    cards: Vec<i32>,
    combo: Combo,
}

#[cfg(feature = "official")]
#[derive(Debug, Clone)]
pub(crate) struct PublicPlay {
    pub position: usize,
    pub cards: Vec<i32>,
    pub benchmark: Vec<i32>,
}

#[cfg(feature = "official")]
#[derive(Debug, Clone)]
pub(crate) struct AutomaticTurn {
    #[allow(dead_code)]
    pub revision: u64,
    pub position: usize,
    pub phase: WhoIsLandlordPhase,
    pub hand: Vec<i32>,
    pub hand_sizes: [usize; PLAYER_COUNT],
    pub self_is_landlord: bool,
    pub bottom_cards: Vec<i32>,
    pub last_play_position: Option<usize>,
    pub last_play: Vec<i32>,
    pub play_history: Vec<PublicPlay>,
    pub intelligent: bool,
}

#[cfg(feature = "official")]
impl AutomaticTurn {
    pub(crate) fn legal_play(&self, cards: &[i32]) -> bool {
        if cards.is_empty()
            || cards
                .iter()
                .any(|card| !self.hand.contains(card))
            || cards.iter().collect::<std::collections::HashSet<_>>().len() != cards.len()
        {
            return false;
        }
        let Some(combo) = classify(cards) else {
            return false;
        };
        self.last_play.is_empty()
            || classify(&self.last_play).is_some_and(|previous| can_beat(combo, previous))
    }

    pub(crate) fn decide(&self) -> WhoIsLandlordAction {
        if self.phase == WhoIsLandlordPhase::BURY {
            return WhoIsLandlordAction::Bury {
                cards: self.hand.iter().rev().take(3).copied().collect(),
            };
        }
        let mut ordered = self.hand.clone();
        ordered.sort_by_key(|card| (card_rank(*card), *card));
        ordered
            .into_iter()
            .find(|card| self.legal_play(&[*card]))
            .map(|card| WhoIsLandlordAction::Play { cards: vec![card] })
            .unwrap_or(WhoIsLandlordAction::Pass)
    }
}

#[derive(Debug, Clone)]
struct PlayerSlot {
    name: String,
    connected: bool,
    away: bool,
    automated: bool,
    intelligent: bool,
    prefer_ai: bool,
    hand: Vec<i32>,
    score: i32,
}

#[derive(Debug)]
pub(crate) struct Room {
    match_id: String,
    players: [PlayerSlot; PLAYER_COUNT],
    landlord_position: usize,
    current_position: usize,
    first_position: usize,
    pub(crate) phase: WhoIsLandlordPhase,
    bottom_cards: Vec<i32>,
    last_play: Option<Play>,
    consecutive_passes: usize,
    bomb_count: i32,
    pub(crate) scores: [i32; PLAYER_COUNT],
    #[cfg(feature = "official")]
    pub(crate) history: Vec<PublicPlay>,
}

impl Room {
    pub(crate) fn new<R: rand::Rng + ?Sized>(id: String, names: [String; 3], rng: &mut R) -> Self {
        let mut deck: Vec<i32> = (1..=DECK_SIZE as i32).collect();
        deck.shuffle(rng);
        let landlord_position = rng.random_range(0..PLAYER_COUNT);
        let first_position = rng.random_range(0..PLAYER_COUNT);
        let players = std::array::from_fn(|position| {
            let mut hand = deck[position * HAND_SIZE..(position + 1) * HAND_SIZE].to_vec();
            if position == landlord_position {
                hand.extend_from_slice(&deck[51..]);
            }
            sort_hand(&mut hand);
            PlayerSlot {
                name: names[position].clone(),
                connected: true,
                away: false,
                automated: false,
                intelligent: false,
                prefer_ai: false,
                hand,
                score: 0,
            }
        });
        Self {
            match_id: id,
            players,
            landlord_position,
            current_position: first_position,
            first_position,
            phase: WhoIsLandlordPhase::BURY,
            bottom_cards: Vec::new(),
            last_play: None,
            consecutive_passes: 0,
            bomb_count: 0,
            scores: [0; 3],
            #[cfg(feature = "official")]
            history: Vec::new(),
        }
    }
    pub(crate) fn sync_presence(&mut self, common: &ws_common::CommonGameState) -> bool {
        let mut changed = false;
        for (position, player) in self.players.iter_mut().enumerate() {
            let connected =
                common.players.contains_key(&position) && !common.is_disconnected(position);
            let away = common.is_away(position) || !connected;
            let automated = cfg!(feature = "official")
                && (common.is_ai_position(position) || common.is_ai_takeover_position(position));
            let intelligent = cfg!(feature = "official")
                && (common.is_ai_position(position)
                    || (common.is_ai_takeover_position(position) && player.prefer_ai));
            changed |= (
                player.connected,
                player.away,
                player.automated,
                player.intelligent,
            ) != (connected, away, automated, intelligent);
            player.connected = connected;
            player.away = away;
            player.automated = automated;
            player.intelligent = intelligent;
        }
        changed
    }
    pub(crate) fn apply(&mut self, position: usize, action: WhoIsLandlordAction) -> bool {
        if position >= PLAYER_COUNT {
            return false;
        }
        if let WhoIsLandlordAction::SetAuto { intelligent } = action {
            if !cfg!(feature = "official") {
                return false;
            }
            self.players[position].prefer_ai = intelligent;
            return true;
        }
        if self.players[position].away
            || self.players[position].automated
            || self.players[position].intelligent
        {
            return false;
        }
        let bury = matches!(action, WhoIsLandlordAction::Bury { .. });
        let valid = match action {
            WhoIsLandlordAction::Bury { cards } => Self::bury_cards(self, position, cards),
            WhoIsLandlordAction::Play { cards }
                if self.phase == WhoIsLandlordPhase::PLAY && position == self.current_position =>
            {
                Self::play_cards(self, position, cards)
            }
            WhoIsLandlordAction::Pass
                if self.phase == WhoIsLandlordPhase::PLAY && position == self.current_position =>
            {
                Self::pass(self, position)
            }
            _ => false,
        };
        if valid && !bury {
            self.advance();
        }
        valid
    }
    pub(crate) fn automatic_step(&mut self) -> bool {
        if self.phase == WhoIsLandlordPhase::BURY {
            let position = self.landlord_position;
            if !self.players[position].away && !self.players[position].automated {
                return false;
            }
            let cards = self.players[position]
                .hand
                .iter()
                .rev()
                .take(3)
                .copied()
                .collect();
            return Self::bury_cards(self, position, cards);
        }
        if self.phase != WhoIsLandlordPhase::PLAY
            || (!self.players[self.current_position].away
                && !self.players[self.current_position].automated)
        {
            return false;
        }
        let position = self.current_position;
        let action = if self.players[position].intelligent {
            smart_brain_action(self, position)
        } else {
            brain_action(self, position)
        };
        match action {
            BrainAction::Play(cards) => {
                Self::play_cards(self, position, cards);
            }
            BrainAction::Pass => {
                Self::pass(self, position);
            }
        }
        self.advance();
        true
    }
    fn advance(&mut self) {
        if let Some(play) = &self.last_play
            && self.players[play.position].hand.is_empty()
        {
            let winner = play.position;
            self.phase = WhoIsLandlordPhase::SETTLEMENT;
            let score = BASE_SCORE.saturating_mul(1_i32 << self.bomb_count.min(20));
            let landlord_won = winner == self.landlord_position;
            for position in 0..PLAYER_COUNT {
                let won = (position == self.landlord_position) == landlord_won;
                let delta = score
                    * if position == self.landlord_position {
                        2
                    } else {
                        1
                    }
                    * if won { 1 } else { -1 };
                self.scores[position] = self.scores[position].saturating_add(delta);
                self.players[position].score = self.scores[position];
            }
        } else if self.consecutive_passes >= PLAYER_COUNT - 1 {
            self.current_position = self
                .last_play
                .as_ref()
                .map(|p| p.position)
                .unwrap_or(self.current_position);
            self.last_play = None;
            self.consecutive_passes = 0;
        } else {
            self.current_position = (self.current_position + 1) % PLAYER_COUNT;
        }
    }
    /// 处理地主埋牌并切换到出牌阶段。
    fn bury_cards(room: &mut Room, position: usize, cards: Vec<i32>) -> bool {
        if room.phase != WhoIsLandlordPhase::BURY || position != room.landlord_position {
            return false;
        }
        let cards = normalize_cards(cards);
        if cards.len() != 3 || !contains_cards(&room.players[position].hand, &cards) {
            return false;
        }
        remove_cards(&mut room.players[position].hand, &cards);
        room.bottom_cards = cards;
        sort_hand(&mut room.players[position].hand);
        room.phase = WhoIsLandlordPhase::PLAY;
        room.current_position = room.first_position;
        true
    }

    /// 验证牌型和压制关系后执行一次出牌。
    fn play_cards(room: &mut Room, position: usize, cards: Vec<i32>) -> bool {
        let cards = normalize_cards(cards);
        let Some(combo) = classify(&cards) else {
            return false;
        };
        if !contains_cards(&room.players[position].hand, &cards) {
            return false;
        }
        if let Some(previous) = room.last_play.as_ref()
            && !can_beat(combo, previous.combo)
        {
            return false;
        }
        #[cfg(feature = "official")]
        let benchmark = room
            .last_play
            .as_ref()
            .map(|play| play.cards.clone())
            .unwrap_or_default();
        remove_cards(&mut room.players[position].hand, &cards);
        if matches!(combo.kind, ComboKind::Bomb | ComboKind::Rocket) {
            room.bomb_count = room.bomb_count.saturating_add(1);
        }
        room.last_play = Some(Play {
            position,
            cards,
            combo,
        });
        #[cfg(feature = "official")]
        if let Some(play) = room.last_play.as_ref() {
            room.history.push(PublicPlay {
                position,
                cards: play.cards.clone(),
                benchmark,
            });
        }
        room.consecutive_passes = 0;
        true
    }

    /// 执行过牌并维护连续过牌计数。
    fn pass(room: &mut Room, position: usize) -> bool {
        if room.last_play.is_none() || room.current_position != position {
            return false;
        }
        #[cfg(feature = "official")]
        let benchmark = room
            .last_play
            .as_ref()
            .map(|play| play.cards.clone())
            .unwrap_or_default();
        room.consecutive_passes = room.consecutive_passes.saturating_add(1);
        #[cfg(feature = "official")]
        room.history.push(PublicPlay {
            position,
            cards: Vec::new(),
            benchmark,
        });
        true
    }
}
impl Room {
    /// 按查看者座位构造不泄露其他玩家手牌的状态快照。
    pub(crate) fn state_for(&self, viewer_position: usize) -> WhoIsLandlordStateEvent {
        let reveal_roles = self.phase == WhoIsLandlordPhase::SETTLEMENT;
        let viewer_is_landlord = viewer_position == self.landlord_position;
        WhoIsLandlordStateEvent {
            match_id: self.match_id.clone(),
            self_position: viewer_position as i32,
            landlord_position: (viewer_is_landlord || reveal_roles)
                .then_some(self.landlord_position as i32),
            current_position: self.current_position as i32,
            phase: self.phase,
            players: self
                .players
                .iter()
                .enumerate()
                .map(|(position, player)| WhoIsLandlordPlayerView {
                    position: position as i32,
                    name: player.name.clone(),
                    // 埋牌结束前统一显示17张，避免从张数泄露地主身份。
                    card_count: if self.phase == WhoIsLandlordPhase::BURY
                        && position != viewer_position
                    {
                        HAND_SIZE as i32
                    } else {
                        player.hand.len() as i32
                    },
                    connected: player.connected,
                    away: player.away,
                    role: if reveal_roles || position == viewer_position {
                        if position == self.landlord_position {
                            WhoIsLandlordRole::LANDLORD
                        } else {
                            WhoIsLandlordRole::FARMER
                        }
                    } else {
                        WhoIsLandlordRole::UNKNOWN
                    },
                })
                .collect(),
            hand: self.players[viewer_position].hand.clone(),
            bottom_cards: if matches!(
                self.phase,
                WhoIsLandlordPhase::PLAY | WhoIsLandlordPhase::SETTLEMENT
            ) {
                self.bottom_cards.clone()
            } else {
                Vec::new()
            },
            last_play_position: self.last_play.as_ref().map(|play| play.position as i32),
            last_play_cards: self
                .last_play
                .as_ref()
                .map(|play| play.cards.clone())
                .unwrap_or_default(),
            multiplier: 1_i32 << self.bomb_count.min(20),
            bomb_count: self.bomb_count,
            round_score: BASE_SCORE.saturating_mul(1_i32 << self.bomb_count.min(20)),
            scores: self.scores.to_vec(),
            auto_intelligent: self.players[viewer_position].prefer_ai,
        }
    }

    #[cfg(feature = "official")]
    fn automatic_turn(&self, position: usize, intelligent: bool) -> AutomaticTurn {
        AutomaticTurn {
            revision: self.play_history_revision(),
            position,
            phase: self.phase,
            hand: self.players[position].hand.clone(),
            hand_sizes: std::array::from_fn(|seat| self.players[seat].hand.len()),
            self_is_landlord: position == self.landlord_position,
            bottom_cards: self.bottom_cards.clone(),
            last_play_position: self.last_play.as_ref().map(|play| play.position),
            last_play: self
                .last_play
                .as_ref()
                .map(|play| play.cards.clone())
                .unwrap_or_default(),
            play_history: self.history.clone(),
            intelligent,
        }
    }

    #[cfg(feature = "official")]
    fn play_history_revision(&self) -> u64 {
        self.history.len() as u64
    }
}

#[derive(Debug)]
enum BrainAction {
    Play(Vec<i32>),
    Pass,
}

type BrainCandidate = ((u8, i32, usize, Vec<i32>), Vec<i32>);

/// 智能托管先尝试一次出完或直接接管当前牌权，再回退到规则正确的普通托管。
/// 该策略只使用本座位手牌和已经公开的上一手牌，保持谁是地主的身份信息隔离。
#[cfg(feature = "official")]
fn smart_brain_action(room: &Room, position: usize) -> BrainAction {
    match crate::ai::choose_action(&room.automatic_turn(position, true)) {
        WhoIsLandlordAction::Play { cards } => BrainAction::Play(cards),
        WhoIsLandlordAction::Pass => BrainAction::Pass,
        WhoIsLandlordAction::Bury { .. } | WhoIsLandlordAction::SetAuto { .. } => {
            brain_action(room, position)
        }
    }
}

#[cfg(not(feature = "official"))]
fn smart_brain_action(room: &Room, position: usize) -> BrainAction {
    brain_action(room, position)
}

/// 为托管座位选择当前阶段可执行的基础 AI 动作。
fn brain_action(room: &Room, position: usize) -> BrainAction {
    let hand = &room.players[position].hand;
    if room.last_play.is_none() {
        return hand
            .iter()
            .min_by_key(|card| (card_rank(**card), **card))
            .copied()
            .map(|card| BrainAction::Play(vec![card]))
            .unwrap_or(BrainAction::Pass);
    }
    let previous_play = room.last_play.as_ref().expect("checked above");
    if previous_play.combo.kind == ComboKind::Rocket {
        return BrainAction::Pass;
    }

    let mut sizes = vec![previous_play.cards.len(), 4, 2];
    sizes.sort_unstable();
    sizes.dedup();
    let mut ordered_hand = hand.clone();
    ordered_hand.sort_by_key(|card| (card_rank(*card), *card));
    let upper_mask = 1_u32 << ordered_hand.len();
    let mut best: Option<BrainCandidate> = None;

    for size in sizes {
        for mask in 1..upper_mask {
            if mask.count_ones() as usize != size {
                continue;
            }
            let cards = ordered_hand
                .iter()
                .enumerate()
                .filter_map(|(index, card)| (mask & (1_u32 << index) != 0).then_some(*card))
                .collect::<Vec<_>>();
            let Some(combo) = classify(&cards) else {
                continue;
            };
            if !can_beat(combo, previous_play.combo) {
                continue;
            }
            let fallback_tier = match combo.kind {
                ComboKind::Rocket => 2,
                ComboKind::Bomb if previous_play.combo.kind != ComboKind::Bomb => 1,
                _ => 0,
            };
            let key = (fallback_tier, combo.main_rank, cards.len(), cards.clone());
            if best.as_ref().is_none_or(|(best_key, _)| key < *best_key) {
                best = Some((key, cards));
            }
        }
    }

    best.map(|(_, cards)| BrainAction::Play(cards))
        .unwrap_or(BrainAction::Pass)
}

/// 过滤无效和重复牌后按固定顺序整理牌列表。
fn normalize_cards(cards: Vec<i32>) -> Vec<i32> {
    let mut cards = cards;
    cards.sort_unstable();
    cards.dedup();
    cards
        .into_iter()
        .filter(|card| (1..=54).contains(card))
        .collect()
}

/// 将手牌按斗地主牌力顺序排列。
fn sort_hand(hand: &mut [i32]) {
    hand.sort_by_key(|card| {
        (
            std::cmp::Reverse(card_rank(*card)),
            std::cmp::Reverse(*card),
        )
    });
}

/// 判断手牌是否包含待使用的全部具体牌值。
fn contains_cards(hand: &[i32], cards: &[i32]) -> bool {
    cards.iter().all(|card| hand.contains(card))
}

/// 从手牌中移除已经打出的牌。
fn remove_cards(hand: &mut Vec<i32>, cards: &[i32]) {
    for card in cards {
        if let Some(index) = hand.iter().position(|candidate| candidate == card) {
            hand.remove(index);
        }
    }
}

/// 将牌编号映射为斗地主比较使用的点数。
fn card_rank(card: i32) -> i32 {
    match card {
        53 => 16,
        54 => 17,
        _ => ((card - 1) % 13) + 3,
    }
}

/// 按点数分组收集牌型识别所需的具体牌值。
fn rank_counts(cards: &[i32]) -> BTreeMap<i32, Vec<i32>> {
    let mut result = BTreeMap::new();
    for card in cards {
        result
            .entry(card_rank(*card))
            .or_insert_with(Vec::new)
            .push(*card);
    }
    result
}

/// 识别一组牌对应的斗地主合法牌型。
fn classify(cards: &[i32]) -> Option<Combo> {
    if cards.is_empty() {
        return None;
    }
    let counts = rank_counts(cards);
    let groups: Vec<(i32, usize)> = counts
        .iter()
        .map(|(rank, cards)| (*rank, cards.len()))
        .collect();
    let len = cards.len();
    if len == 2
        && counts.get(&16).is_some_and(|cards| cards.len() == 1)
        && counts.get(&17).is_some_and(|cards| cards.len() == 1)
    {
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
    if len == 4 && groups.len() == 2 && groups.iter().any(|(_, count)| *count == 3) {
        return Some(Combo {
            kind: ComboKind::TripleSingle,
            main_rank: groups.iter().find(|(_, count)| *count == 3)?.0,
            sequence_len: 1,
        });
    }
    if len == 5
        && groups.len() == 2
        && groups.iter().any(|(_, count)| *count == 3)
        && groups.iter().any(|(_, count)| *count == 2)
    {
        return Some(Combo {
            kind: ComboKind::TriplePair,
            main_rank: groups.iter().find(|(_, count)| *count == 3)?.0,
            sequence_len: 1,
        });
    }
    let single_ranks: Vec<i32> = groups
        .iter()
        .filter(|(_, count)| *count == 1)
        .map(|(rank, _)| *rank)
        .collect();
    if len >= 5
        && single_ranks.len() == len
        && single_ranks.iter().all(|rank| (3..=14).contains(rank))
        && consecutive(&single_ranks)
    {
        return Some(Combo {
            kind: ComboKind::Straight,
            main_rank: *single_ranks.last()?,
            sequence_len: len,
        });
    }
    let pair_ranks: Vec<i32> = groups
        .iter()
        .filter(|(_, count)| *count == 2)
        .map(|(rank, _)| *rank)
        .collect();
    if len >= 6
        && len.is_multiple_of(2)
        && pair_ranks.len() * 2 == len
        && pair_ranks.iter().all(|rank| (3..=14).contains(rank))
        && consecutive(&pair_ranks)
    {
        return Some(Combo {
            kind: ComboKind::StraightPairs,
            main_rank: *pair_ranks.last()?,
            sequence_len: pair_ranks.len(),
        });
    }
    let triple_ranks: Vec<i32> = groups
        .iter()
        .filter(|(_, count)| *count == 3)
        .map(|(rank, _)| *rank)
        .collect();
    if triple_ranks.len() >= 2
        && triple_ranks.iter().all(|rank| (3..=14).contains(rank))
        && consecutive(&triple_ranks)
    {
        let sequence_len = triple_ranks.len();
        let main_rank = *triple_ranks.last()?;
        if len == sequence_len * 3 {
            return Some(Combo {
                kind: ComboKind::Plane,
                main_rank,
                sequence_len,
            });
        }
        if len == sequence_len * 4
            && groups
                .iter()
                .filter(|(rank, count)| *count == 1 && !triple_ranks.contains(rank))
                .count()
                == sequence_len
        {
            return Some(Combo {
                kind: ComboKind::PlaneWithSingles,
                main_rank,
                sequence_len,
            });
        }
        if len == sequence_len * 5
            && groups
                .iter()
                .filter(|(rank, count)| *count == 2 && !triple_ranks.contains(rank))
                .count()
                == sequence_len
        {
            return Some(Combo {
                kind: ComboKind::PlaneWithPairs,
                main_rank,
                sequence_len,
            });
        }
    }
    if len == 6 && groups.iter().any(|(_, count)| *count == 4) {
        return Some(Combo {
            kind: ComboKind::FourWithTwoSingles,
            main_rank: groups.iter().find(|(_, count)| *count == 4)?.0,
            sequence_len: 1,
        });
    }
    if len == 8
        && groups.iter().any(|(_, count)| *count == 4)
        && groups.iter().filter(|(_, count)| *count == 2).count() == 2
    {
        return Some(Combo {
            kind: ComboKind::FourWithTwoPairs,
            main_rank: groups.iter().find(|(_, count)| *count == 4)?.0,
            sequence_len: 1,
        });
    }
    None
}

/// 判断点数序列是否严格连续。
fn consecutive(ranks: &[i32]) -> bool {
    ranks.windows(2).all(|window| window[1] == window[0] + 1)
}

/// 判断当前牌型是否能压过上一手牌。
fn can_beat(current: Combo, previous: Combo) -> bool {
    if current.kind == ComboKind::Rocket {
        return previous.kind != ComboKind::Rocket;
    }
    if current.kind == ComboKind::Bomb {
        if previous.kind == ComboKind::Rocket {
            return false;
        }
        if previous.kind == ComboKind::Bomb {
            return current.main_rank > previous.main_rank;
        }
        return true;
    }
    if matches!(previous.kind, ComboKind::Rocket | ComboKind::Bomb) {
        return false;
    }
    current.kind == previous.kind
        && current.sequence_len == previous.sequence_len
        && current.main_rank > previous.main_rank
}

#[cfg(test)]
#[path = "core/tests.rs"]
mod tests;
