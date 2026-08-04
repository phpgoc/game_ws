use std::fmt;

use crate::MAX_DECK_COUNT;

const CARDS_PER_DECK: u8 = 54;
const CARD_ID_STRIDE: i32 = 100;

/// 一张带牌副编号的实体牌。
///
/// 保留现有协议编码：第一副为 `1..=54`，之后每副依次增加 100。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Card {
    encoded: i32,
    identity: u8,
    deck_index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardDecodeError {
    NonPositive(i32),
    InvalidIdentity(i32),
    TooManyDecks(u32),
}

impl fmt::Display for CardDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositive(value) => write!(formatter, "card id must be positive: {value}"),
            Self::InvalidIdentity(value) => {
                write!(formatter, "card id has an invalid identity: {value}")
            }
            Self::TooManyDecks(deck_index) => write!(
                formatter,
                "card id uses deck index {deck_index}, but at most {MAX_DECK_COUNT} decks are supported"
            ),
        }
    }
}

impl std::error::Error for CardDecodeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Suit {
    Spade = 0,
    Heart = 1,
    Club = 2,
    Diamond = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
    SmallJoker = 16,
    BigJoker = 17,
}

impl Card {
    pub fn decode(encoded: i32) -> Result<Self, CardDecodeError> {
        if encoded <= 0 {
            return Err(CardDecodeError::NonPositive(encoded));
        }

        let deck_index = ((encoded - 1) / CARD_ID_STRIDE) as u32;
        if deck_index >= u32::from(MAX_DECK_COUNT) {
            return Err(CardDecodeError::TooManyDecks(deck_index));
        }

        let identity = ((encoded - 1) % CARD_ID_STRIDE + 1) as u8;
        if identity > CARDS_PER_DECK {
            return Err(CardDecodeError::InvalidIdentity(encoded));
        }

        Ok(Self {
            encoded,
            identity,
            deck_index: deck_index as u8,
        })
    }

    pub const fn encoded(self) -> i32 {
        self.encoded
    }

    pub const fn identity(self) -> u8 {
        self.identity
    }

    pub const fn deck_index(self) -> u8 {
        self.deck_index
    }

    pub fn rank(self) -> Rank {
        match self.identity {
            53 => Rank::SmallJoker,
            54 => Rank::BigJoker,
            identity => match (identity - 1) % 13 + 2 {
                2 => Rank::Two,
                3 => Rank::Three,
                4 => Rank::Four,
                5 => Rank::Five,
                6 => Rank::Six,
                7 => Rank::Seven,
                8 => Rank::Eight,
                9 => Rank::Nine,
                10 => Rank::Ten,
                11 => Rank::Jack,
                12 => Rank::Queen,
                13 => Rank::King,
                14 => Rank::Ace,
                _ => unreachable!("standard card rank is always in 2..=14"),
            },
        }
    }

    pub fn suit(self) -> Option<Suit> {
        match self.identity {
            1..=13 => Some(Suit::Spade),
            14..=26 => Some(Suit::Heart),
            27..=39 => Some(Suit::Club),
            40..=52 => Some(Suit::Diamond),
            53..=54 => None,
            _ => unreachable!("decoded card identity is always in 1..=54"),
        }
    }

    pub fn points(self) -> u8 {
        match self.rank() {
            Rank::Five => 5,
            Rank::Ten | Rank::King => 10,
            _ => 0,
        }
    }
}

impl TryFrom<i32> for Card {
    type Error = CardDecodeError;

    fn try_from(encoded: i32) -> Result<Self, Self::Error> {
        Self::decode(encoded)
    }
}

impl From<Card> for i32 {
    fn from(card: Card) -> Self {
        card.encoded()
    }
}

/// 返回一组牌中相同牌面身份的最大张数。
///
/// 这里只提供频数原语，不决定甩牌是否合法，也不直接计算扣底倍率。
pub fn largest_identity_group_size(cards: &[Card]) -> usize {
    let mut counts = [0_usize; CARDS_PER_DECK as usize + 1];
    for card in cards {
        counts[card.identity() as usize] += 1;
    }
    counts.into_iter().max().unwrap_or_default()
}
