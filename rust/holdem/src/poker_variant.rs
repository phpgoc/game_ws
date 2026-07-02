use share_type_public::GameId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PokerHandRule {
    BestFiveAny,
    OmahaTwoHoleThreeBoard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PokerVariant {
    pub game_id: GameId,
    pub hole_cards: usize,
    pub open_hole_cards: usize,
    pub min_card: i32,
    pub hand_rule: PokerHandRule,
}

pub const STANDARD_TEXAS: PokerVariant = PokerVariant {
    game_id: GameId::TEXAS_HOLD_EM,
    hole_cards: 2,
    open_hole_cards: 0,
    min_card: 1,
    hand_rule: PokerHandRule::BestFiveAny,
};

pub const OPEN_HOLD_EM: PokerVariant = PokerVariant {
    game_id: GameId::OPEN_HOLD_EM,
    hole_cards: 3,
    open_hole_cards: 1,
    min_card: 1,
    hand_rule: PokerHandRule::BestFiveAny,
};

pub const SHORT_DECK_HOLD_EM: PokerVariant = PokerVariant {
    game_id: GameId::SHORT_DECK_HOLD_EM,
    hole_cards: 2,
    open_hole_cards: 0,
    min_card: 6,
    hand_rule: PokerHandRule::BestFiveAny,
};

pub const OMAHA_HOLD_EM: PokerVariant = PokerVariant {
    game_id: GameId::OMAHA_HOLD_EM,
    hole_cards: 4,
    open_hole_cards: 0,
    min_card: 1,
    hand_rule: PokerHandRule::OmahaTwoHoleThreeBoard,
};

pub const POKER_VARIANTS: [PokerVariant; 4] = [
    STANDARD_TEXAS,
    OPEN_HOLD_EM,
    SHORT_DECK_HOLD_EM,
    OMAHA_HOLD_EM,
];

pub fn accepts_poker_game_id(game_id: GameId) -> bool {
    variant_for_game_id(game_id).is_some()
}

pub fn variant_for_game_id(game_id: GameId) -> Option<PokerVariant> {
    POKER_VARIANTS
        .iter()
        .copied()
        .find(|variant| variant.game_id == game_id)
}

impl PokerVariant {
    pub fn hidden_hole_cards(self) -> usize {
        self.hole_cards.saturating_sub(self.open_hole_cards)
    }

    pub fn public_hole_cards(self, cards: &[i32]) -> Vec<i32> {
        let hidden = self.hidden_hole_cards();
        cards.iter().copied().skip(hidden).collect()
    }
}
