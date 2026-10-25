use rand::seq::SliceRandom;
use share_type_public::games::niangniang::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
struct Player {
    name: String,
    hand: Vec<i32>,
    connected: bool,
    away: bool,
    intelligent: bool,
    prefer_ai: bool,
}
#[derive(Debug)]
pub(crate) struct Room {
    id: String,
    players: [Player; 4],
    pub(crate) phase: NiangniangPhase,
    decks: i32,
    round: i32,
    dealer: usize,
    levels: [i32; 2],
    level: i32,
    current: usize,
    last: Option<(usize, Vec<i32>)>,
    passed: Vec<usize>,
    finished: Vec<usize>,
    tributes: Vec<NiangniangTribute>,
    result: Option<NiangniangRoundResult>,
    winner: Option<usize>,
    #[cfg(feature = "official")]
    history: Vec<crate::ai_input::PublicTurn>,
}
pub(crate) fn rank(card: i32) -> i32 {
    let id = (card - 1).rem_euclid(54) + 1;
    if id == 53 {
        16
    } else if id == 54 {
        17
    } else {
        (id - 1) % 13 + 3
    }
}
fn strength(card: i32, level: i32) -> i32 {
    let r = rank(card);
    if r == level {
        16
    } else if r >= 16 {
        r + 1
    } else {
        r
    }
}
impl Room {
    pub(crate) fn new<R: rand::Rng + ?Sized>(
        id: String,
        names: [String; 4],
        decks: i32,
        rng: &mut R,
    ) -> Self {
        let mut room = Self {
            id,
            players: std::array::from_fn(|p| Player {
                name: names[p].clone(),
                hand: Vec::new(),
                connected: true,
                away: false,
                intelligent: false,
                prefer_ai: false,
            }),
            phase: NiangniangPhase::PLAY,
            decks: decks.clamp(1, 2),
            round: 0,
            dealer: 0,
            levels: [3, 3],
            level: 3,
            current: 0,
            last: None,
            passed: Vec::new(),
            finished: Vec::new(),
            tributes: Vec::new(),
            result: None,
            winner: None,
            #[cfg(feature = "official")]
            history: Vec::new(),
        };
        room.deal(rng);
        room
    }
    fn deal<R: rand::Rng + ?Sized>(&mut self, rng: &mut R) {
        self.round += 1;
        self.level = self.levels[self.dealer % 2];
        self.current = self.dealer;
        self.phase = NiangniangPhase::PLAY;
        self.last = None;
        self.passed.clear();
        self.finished.clear();
        self.tributes.clear();
        #[cfg(feature = "official")]
        self.history.clear();
        for player in &mut self.players {
            player.hand.clear();
        }
        let mut cards: Vec<i32> = (1..=54 * self.decks).collect();
        cards.shuffle(rng);
        for (i, card) in cards.into_iter().enumerate() {
            self.players[(self.dealer + i) % 4].hand.push(card);
        }
        // 每组贡牌在换牌前取极值；两组涉及的座位互不重叠。
        let count = self.result.as_ref().map_or(0, |r| r.tribute_count);
        for index in 0..count {
            let to = (self.dealer + index as usize * 2) % 4;
            let from = (to + 3) % 4;
            let tribute = *self.players[from]
                .hand
                .iter()
                .max_by_key(|c| (strength(**c, self.level), **c))
                .unwrap();
            let returned = *self.players[to]
                .hand
                .iter()
                .min_by_key(|c| (strength(**c, self.level), **c))
                .unwrap();
            self.players[from].hand.retain(|c| *c != tribute);
            self.players[to].hand.retain(|c| *c != returned);
            self.players[from].hand.push(returned);
            self.players[to].hand.push(tribute);
            self.tributes.push(NiangniangTribute {
                from_position: from as i32,
                to_position: to as i32,
                tribute_card: tribute,
                return_card: returned,
            });
        }
        for player in &mut self.players {
            player.hand.sort_by_key(|c| (strength(*c, self.level), *c));
        }
    }
    pub(crate) fn next_round<R: rand::Rng + ?Sized>(&mut self, rng: &mut R) -> bool {
        if self.phase != NiangniangPhase::ROUND_OVER {
            return false;
        }
        self.dealer = self.result.as_ref().unwrap().next_dealer_position as usize;
        self.deal(rng);
        true
    }
    pub(crate) fn sync_presence(&mut self, common: &ws_common::CommonGameState) -> bool {
        let mut changed = false;
        for (p, player) in self.players.iter_mut().enumerate() {
            let connected = common.players.contains_key(&p) && !common.is_disconnected(p);
            let away = common.is_away(p) || !connected;
            let intelligent = cfg!(feature = "official")
                && (common.is_ai_position(p)
                    || (common.is_ai_takeover_position(p) && player.prefer_ai));
            changed |= (player.connected, player.away, player.intelligent)
                != (connected, away, intelligent);
            player.connected = connected;
            player.away = away;
            player.intelligent = intelligent;
        }
        changed
    }
    pub(crate) fn apply(&mut self, p: usize, action: NiangniangAction) -> bool {
        if let NiangniangAction::SetAuto { intelligent } = action {
            if p >= 4 || !cfg!(feature = "official") {
                return false;
            }
            self.players[p].prefer_ai = intelligent;
            return true;
        }
        if p >= 4 || self.players[p].away || self.players[p].intelligent {
            return false;
        }
        self.apply_turn(p, action)
    }
    fn apply_turn(&mut self, p: usize, action: NiangniangAction) -> bool {
        if self.phase != NiangniangPhase::PLAY || p != self.current || self.finished.contains(&p) {
            return false;
        }
        match action {
            NiangniangAction::Play { cards } => {
                let hand = &self.players[p].hand;
                if cards.is_empty()
                    || cards
                        .iter()
                        .any(|c| !hand.contains(c) || rank(*c) != rank(cards[0]))
                    || cards.iter().collect::<std::collections::HashSet<_>>().len() != cards.len()
                {
                    return false;
                }
                if let Some((_, previous)) = &self.last
                    && (cards.len() != previous.len()
                        || strength(cards[0], self.level) <= strength(previous[0], self.level))
                {
                    return false;
                }
                #[cfg(feature = "official")]
                self.history.push(crate::ai_input::PublicTurn::Play {
                    position: p,
                    cards: cards.clone(),
                });
                self.players[p].hand.retain(|c| !cards.contains(c));
                self.last = Some((p, cards));
                self.passed.clear();
                if self.players[p].hand.is_empty() {
                    self.finished.push(p);
                    if self.finished.len() >= 2 {
                        let first = self.finished[0];
                        if self.finished.contains(&((first + 2) % 4)) || self.finished.len() == 3 {
                            self.finish_round();
                            return true;
                        }
                    }
                }
            }
            NiangniangAction::SetAuto { .. } => return false,
            NiangniangAction::Pass => {
                if self.last.is_none() {
                    return false;
                }
                self.passed.push(p);
                #[cfg(feature = "official")]
                self.history
                    .push(crate::ai_input::PublicTurn::Pass { position: p });
            }
        }
        self.advance_turn(p);
        true
    }
    fn advance_turn(&mut self, p: usize) {
        let next = (1..=4)
            .map(|i| (p + i) % 4)
            .find(|p| !self.finished.contains(p))
            .unwrap();
        let owner = self.last.as_ref().unwrap().0;
        // 每位仍持牌的非领牌者都已让过，或轮转回持有最大牌的人。
        if next == owner
            || (0..4).all(|p| p == owner || self.finished.contains(&p) || self.passed.contains(&p))
        {
            self.last = None;
            self.passed.clear();
            self.current = if !self.finished.contains(&owner) {
                owner
            } else if !self.finished.contains(&((owner + 2) % 4)) {
                (owner + 2) % 4
            } else {
                next
            };
        } else {
            self.current = next;
        }
    }
    fn finish_round(&mut self) {
        let first = self.finished[0];
        let partner = (first + 2) % 4;
        if self.finished.len() == 3 && !self.finished.contains(&partner) {
            self.finished.push(partner);
        }
        let place = self.finished.iter().position(|p| *p == partner).unwrap();
        let tribute_count = match place {
            1 => 2,
            2 => 1,
            _ => 0,
        };
        let dealer_won = first % 2 == self.dealer % 2;
        let advanced = if dealer_won { tribute_count } else { 0 };
        if dealer_won && self.level == 14 && advanced > 0 {
            self.winner = Some(first % 2);
            self.phase = NiangniangPhase::MATCH_OVER;
        } else {
            if dealer_won {
                self.levels[first % 2] = (self.level + advanced).min(14);
            }
            self.phase = NiangniangPhase::ROUND_OVER;
        }
        self.result = Some(NiangniangRoundResult {
            winning_team: (first % 2) as i32,
            dealer_won,
            advanced,
            tribute_count,
            next_dealer_position: ((self.dealer + if dealer_won { 2 } else { 1 }) % 4) as i32,
            finish_order: self.finished.iter().map(|p| *p as i32).collect(),
        });
    }
    fn choose(&self, p: usize, intelligent: bool) -> NiangniangAction {
        #[cfg(feature = "official")]
        if intelligent {
            let observation = self.ai_observation(p);
            if let Some(action) = crate::ai::choose(&observation) {
                return action;
            }
        }
        self.choose_fallback(p, intelligent)
    }
    #[cfg(feature = "official")]
    fn ai_observation(&self, p: usize) -> crate::ai_input::AiObservation<'_> {
        crate::ai_input::AiObservation {
            state: self.state_for(p),
            history: &self.history,
        }
    }
    /// 普通托管及数据不完整时的回退，仅使用本人手牌和公开状态。
    fn choose_fallback(&self, p: usize, intelligent: bool) -> NiangniangAction {
        let mut groups = BTreeMap::<i32, Vec<i32>>::new();
        for card in &self.players[p].hand {
            groups
                .entry(strength(*card, self.level))
                .or_default()
                .push(*card);
        }
        if let Some((owner, previous)) = &self.last {
            if intelligent && owner % 2 == p % 2 && !self.finished.contains(owner) {
                // 能一次出完时优先出完，否则让队友继续领牌。
                if let Some(cards) = groups.values().find(|g| {
                    g.len() == self.players[p].hand.len()
                        && g.len() == previous.len()
                        && strength(g[0], self.level) > strength(previous[0], self.level)
                }) {
                    return NiangniangAction::Play {
                        cards: cards.clone(),
                    };
                }
                return NiangniangAction::Pass;
            }
            let candidates = groups.iter().filter(|(r, g)| {
                **r > strength(previous[0], self.level) && g.len() >= previous.len()
            });
            let choice = if intelligent {
                candidates.min_by_key(|(r, g)| (g.len() != previous.len(), **r))
            } else {
                candidates.into_iter().next()
            };
            if let Some((_, cards)) = choice {
                return NiangniangAction::Play {
                    cards: cards[..previous.len()].to_vec(),
                };
            }
            NiangniangAction::Pass
        } else {
            let cards = if intelligent {
                groups
                    .iter()
                    .min_by_key(|(r, g)| (-(g.len() as i32), **r))
                    .unwrap()
                    .1
            } else {
                groups.first_key_value().unwrap().1
            };
            NiangniangAction::Play {
                cards: cards.clone(),
            }
        }
    }
    pub(crate) fn automatic_step(&mut self) -> bool {
        if self.phase != NiangniangPhase::PLAY {
            return false;
        }
        let p = self.current;
        if !(self.players[p].away || self.players[p].intelligent) {
            return false;
        }
        self.apply_turn(p, self.choose(p, self.players[p].intelligent))
    }
    pub(crate) fn state_for(&self, p: usize) -> NiangniangStateEvent {
        NiangniangStateEvent {
            match_id: self.id.clone(),
            self_position: p as i32,
            phase: self.phase,
            round_number: self.round,
            deck_count: self.decks,
            dealer_position: self.dealer as i32,
            team_levels: self.levels.to_vec(),
            level_rank: self.level,
            current_position: self.current as i32,
            players: self
                .players
                .iter()
                .enumerate()
                .map(|(p, player)| NiangniangPlayerView {
                    position: p as i32,
                    name: player.name.clone(),
                    card_count: player.hand.len() as i32,
                    connected: player.connected,
                    away: player.away,
                })
                .collect(),
            auto_intelligent: self.players[p].prefer_ai,
            hand: self.players[p].hand.clone(),
            last_play_position: self.last.as_ref().map(|(p, _)| *p as i32),
            last_play_cards: self
                .last
                .as_ref()
                .map_or_else(Vec::new, |(_, cards)| cards.clone()),
            passed_positions: self.passed.iter().map(|p| *p as i32).collect(),
            finish_order: self.finished.iter().map(|p| *p as i32).collect(),
            tributes: self.tributes.clone(),
            round_result: self.result.clone(),
            match_winner: self.winner.map(|t| t as i32),
        }
    }
}
#[cfg(test)]
mod tests;
