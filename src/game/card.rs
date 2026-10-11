#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Rank {
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Two,
    Ace,
}

impl Rank {
    pub const COUNT: usize = 13;
    pub const ALL: [Rank; Self::COUNT] = [
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
        Rank::Two,
        Rank::Ace,
    ];
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Suit {
    Heart,
    Diamond,
    Spade,
    Club,
}

impl Suit {
    pub const COUNT: usize = 4;
    pub const ALL: [Suit; Self::COUNT] = [Suit::Heart, Suit::Diamond, Suit::Spade, Suit::Club];
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Card {
    suit: Suit,
    rank: Rank,
}

impl Card {
    pub const DEFAULT: Self = Self {
        suit: Suit::Heart,
        rank: Rank::Three,
    };

    pub const COUNT: usize = Suit::COUNT * Rank::COUNT;
    pub const ALL: [Card; Self::COUNT] = {
        let mut out = [Card::DEFAULT; Self::COUNT];
        let mut i = 0;
        let mut suit = 0;
        while suit < Suit::COUNT {
            let mut rank = 0;
            while rank < Rank::COUNT {
                out[i] = Card {
                    suit: Suit::ALL[suit],
                    rank: Rank::ALL[rank],
                };
                i += 1;
                rank += 1;
            }

            suit += 1;
        }
        out
    };
}
