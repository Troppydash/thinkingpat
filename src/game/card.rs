use rand::rngs::ThreadRng;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Rank {
    Two,
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
    Ace,
}

impl Rank {
    pub const COUNT: usize = 13;
    pub const ALL: [Rank; Self::COUNT] = [
        Rank::Two,
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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Card(u64);

impl Card {
    pub const DEFAULT: Self = Self(0);

    pub const fn new(suit: Suit, rank: Rank) -> Self {
        Self(suit as u64 + ((rank as u64) << 2))
    }

    pub const fn suit(&self) -> Suit {
        Suit::ALL[(self.0 & 0b11) as usize]
    }

    pub const fn rank(&self) -> Rank {
        Rank::ALL[(self.0 >> 2) as usize]
    }

    pub const COUNT: usize = Suit::COUNT * Rank::COUNT;
    pub const ALL: [Card; Self::COUNT] = {
        let mut out = [Card::DEFAULT; Self::COUNT];
        let mut i = 0;
        let mut suit = 0;
        while suit < Suit::COUNT {
            let mut rank = 0;
            while rank < Rank::COUNT {
                out[i] = Card::new(Suit::ALL[suit], Rank::ALL[rank]);
                i += 1;
                rank += 1;
            }

            suit += 1;
        }
        out
    };

    pub fn parse_card(text: &str) -> Card {
        let rank = match text.as_bytes()[0] {
            b'2' => Rank::Two,
            b'3' => Rank::Three,
            b'4' => Rank::Four,
            b'5' => Rank::Five,
            b'6' => Rank::Six,
            b'7' => Rank::Seven,
            b'8' => Rank::Eight,
            b'9' => Rank::Nine,
            b'T' => Rank::Ten,
            b'J' => Rank::Jack,
            b'Q' => Rank::Queen,
            b'K' => Rank::King,
            b'A' => Rank::Ace,
            _ => panic!("invalid rank: {text}"),
        };
        let suit = match text.as_bytes()[1] {
            b'h' => Suit::Heart,
            b'd' => Suit::Diamond,
            b's' => Suit::Spade,
            b'c' => Suit::Club,
            _ => panic!("invalid suit: {text}"),
        };
        Card::new(suit, rank)
    }

    pub fn parse_cards(text: &str) -> Vec<Card> {
        let cards: Vec<_> = text.split_whitespace().map(Card::parse_card).collect();
        cards
    }

    fn sample_remain(cards: &[Card], rng: &ThreadRng, n: usize) -> Vec<Card> {
        unimplemented!()
    }
}
