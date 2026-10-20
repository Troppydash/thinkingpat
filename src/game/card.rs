use std::{
    iter::FusedIterator,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Sub, SubAssign},
};

use rand::{Rng, RngExt, rngs::ThreadRng, seq::index};

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
#[repr(transparent)]
pub struct Card(pub u32);

impl Card {
    pub const DEFAULT: Self = Self(0);

    pub const fn new(suit: Suit, rank: Rank) -> Self {
        Self(suit as u32 + ((rank as u32) << 2))
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
}

#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Copy)]
#[repr(transparent)]
pub struct CardSet(pub u64);

impl CardSet {
    pub const EMPTY: Self = Self(0);
    pub const FULL: Self = Self((1u64 << Card::COUNT) - 1);

    #[inline]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits & Self::FULL.0)
    }

    #[inline]
    pub const fn bits(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn contains(self, card: Card) -> bool {
        self.0 & (1u64 << card.0) != 0
    }

    #[inline]
    pub const fn is_disjoint(self, other: Self) -> bool {
        self.0 & other.0 == 0
    }

    #[inline]
    pub const fn is_subset(self, other: Self) -> bool {
        self.0 & other.0 == self.0
    }

    #[inline]
    pub fn insert(&mut self, card: Card) -> bool {
        let bit = 1u64 << card.0;
        let added = self.0 & bit == 0;
        self.0 |= bit;
        added
    }

    #[inline]
    pub fn remove(&mut self, card: Card) -> bool {
        let bit = 1u64 << card.0;
        let removed = self.0 & bit != 0;
        self.0 &= !bit;
        removed
    }

    /// Gather this suit's interleaved bits into a 13-bit rank mask.
    #[cfg(target_feature = "bmi2")]
    #[inline]
    pub unsafe fn suit_ranks(self, suit: Suit) -> u16 {
        let mask = 0x0001_1111_1111_1111u64 << suit as u32;
        std::arch::x86_64::_pext_u64(self.0, mask) as u16
    }

    #[cfg(not(target_feature = "bmi2"))]
    #[inline]
    pub const fn suit_ranks(self, suit: Suit) -> u16 {
        let mut bits = (self.0 >> suit as u32) & 0x0001_1111_1111_1111;
        // i don't understand this bitmask LLM magic but the bmi2 version is easier anyways
        bits = (bits | (bits >> 3)) & 0x0303_0303_0303_0303;
        bits = (bits | (bits >> 6)) & 0x000f_000f_000f_000f;
        bits = (bits | (bits >> 12)) & 0x0000_00ff_0000_00ff;
        (bits | (bits >> 24)) as u16
    }

    #[inline]
    pub fn iter(self) -> CardSetIter {
        CardSetIter(self.0)
    }

    pub fn parse(text: &str) -> Self {
        text.split_whitespace().map(Card::parse_card).collect()
    }

    pub fn sample<R: Rng>(rng: &mut R, n: usize) -> CardSet {
        Self::sample_remain(CardSet::EMPTY, rng, n)
    }

    pub fn sample_remain<R: Rng>(cards: CardSet, rng: &mut R, n: usize) -> CardSet {
        let mut i = 0;
        let mut selected = CardSet::EMPTY;
        while i < n {
            let card = Card(rng.random_range(0..Card::COUNT) as u32);
            if (selected | cards).contains(card) {
                continue;
            }
            selected.insert(card);
            i += 1;
        }

        selected
    }

    pub fn sample_remain_ordered<const N: usize, R: Rng>(cards: CardSet, rng: &mut R) -> [Card; N] {
        let mut out = [Card::DEFAULT; N];
        let mut i = 0;
        let mut found = cards;
        while i < N {
            let card = Card(rng.random_range(0..Card::COUNT) as u32);
            if found.contains(card) {
                continue;
            }
            found.insert(card);
            out[i] = card;
            i += 1;
        }

        out
    }
}

impl From<Card> for CardSet {
    #[inline]
    fn from(card: Card) -> Self {
        Self(1u64 << card.0)
    }
}

impl<const N: usize> From<[Card; N]> for CardSet {
    fn from(cards: [Card; N]) -> Self {
        cards.into_iter().collect()
    }
}

impl FromIterator<Card> for CardSet {
    fn from_iter<T: IntoIterator<Item = Card>>(cards: T) -> Self {
        let mut set = Self::EMPTY;
        set.extend(cards);
        set
    }
}

impl Extend<Card> for CardSet {
    fn extend<T: IntoIterator<Item = Card>>(&mut self, cards: T) {
        for card in cards {
            self.insert(card);
        }
    }
}

macro_rules! bit_op {
    ($trait:ident, $method:ident, $assign:ident, $assign_method:ident, $op:tt) => {
        impl $trait for CardSet {
            type Output = Self;
            #[inline]
            fn $method(self, rhs: Self) -> Self { Self(self.0 $op rhs.0) }
        }
        impl $assign for CardSet {
            #[inline]
            fn $assign_method(&mut self, rhs: Self) { *self = *self $op rhs; }
        }
    };
}
bit_op!(BitOr, bitor, BitOrAssign, bitor_assign, |);
bit_op!(BitAnd, bitand, BitAndAssign, bitand_assign, &);
bit_op!(BitXor, bitxor, BitXorAssign, bitxor_assign, ^);

impl Not for CardSet {
    type Output = Self;
    #[inline]
    fn not(self) -> Self {
        Self(self.0 ^ Self::FULL.0)
    }
}

impl Sub for CardSet {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 & !rhs.0)
    }
}

impl SubAssign for CardSet {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 &= !rhs.0;
    }
}

/// Visits only set bits, in ascending card order, without allocating.
#[derive(Debug, Clone)]
pub struct CardSetIter(u64);

impl Iterator for CardSetIter {
    type Item = Card;

    #[inline]
    fn next(&mut self) -> Option<Card> {
        if self.0 == 0 {
            return None;
        }
        let card = Card(self.0.trailing_zeros());
        self.0 &= self.0 - 1;
        Some(card)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count_ones() as usize;
        (len, Some(len))
    }
}

impl DoubleEndedIterator for CardSetIter {
    #[inline]
    fn next_back(&mut self) -> Option<Card> {
        if self.0 == 0 {
            return None;
        }
        let card = Card(63 - self.0.leading_zeros());
        self.0 &= !(1u64 << card.0);
        Some(card)
    }
}

impl ExactSizeIterator for CardSetIter {}
impl FusedIterator for CardSetIter {}

impl IntoIterator for CardSet {
    type Item = Card;
    type IntoIter = CardSetIter;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suit_rank_paths_match_all_rank_patterns() {
        for suit in Suit::ALL {
            let suit_mask = 0x0001_1111_1111_1111u64 << suit as u32;
            for ranks in 0u16..(1 << Rank::COUNT) {
                // Fill other suits too, so they must be ignored by both paths.
                let mut cards = CardSet::FULL - CardSet::from_bits(suit_mask);
                for rank in Rank::ALL {
                    if ranks & (1 << rank as u32) != 0 {
                        cards.insert(Card::new(suit, rank));
                    }
                }
                assert_eq!(cards.suit_ranks(suit), ranks);
            }
        }
    }

    #[test]
    fn bits_match_card_encoding_and_suit_masks() {
        assert_eq!(size_of::<CardSet>(), size_of::<u64>());
        for card in Card::ALL {
            let set = CardSet::from(card);
            assert_eq!(set.bits(), 1u64 << card.0);
            assert_eq!(set.iter().collect::<Vec<_>>(), [card]);
            for suit in Suit::ALL {
                assert_eq!(
                    set.suit_ranks(suit),
                    if suit == card.suit() {
                        1 << card.rank() as u32
                    } else {
                        0
                    }
                );
            }
        }
        assert_eq!(CardSet::from_bits(u64::MAX), CardSet::FULL);
        assert_eq!(CardSet::FULL.len(), 52);
        for suit in Suit::ALL {
            assert_eq!(CardSet::FULL.suit_ranks(suit), 0x1fff);
        }
    }

    #[test]
    fn set_operations_and_iteration() {
        let a = CardSet::parse("Ah Kd 2c Ah");
        let b = CardSet::parse("Ah Qs");
        assert_eq!(a.len(), 3);
        assert_eq!(a & b, CardSet::parse("Ah"));
        assert_eq!(a | b, CardSet::parse("Ah Kd 2c Qs"));
        assert_eq!(a ^ b, CardSet::parse("Kd 2c Qs"));
        assert_eq!(a - b, CardSet::parse("Kd 2c"));
        assert!((a & b).is_subset(a));
        assert!((!a).is_disjoint(a));
        assert_eq!(a | !a, CardSet::FULL);
        assert!((a & !a).is_empty());
        let mut set = a;
        set |= b;
        set &= a;
        set ^= b;
        set -= b;
        assert_eq!(set, a - b);
        let ace = Card::parse_card("Ah");
        assert!(set.insert(ace));
        assert!(!set.insert(ace));
        assert!(set.contains(ace));
        assert!(set.remove(ace));
        assert!(!set.remove(ace));
        assert_eq!(
            a.iter().rev().collect::<Vec<_>>(),
            a.iter()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
        );
        assert_eq!(a.iter().collect::<CardSet>(), a);
    }

    #[test]
    fn sampling_excludes_known_cards() {
        let known = CardSet::parse("Ah Kd 2c");
        let mut rng = rand::rng();
        for n in [0, 7, 49] {
            let sampled = CardSet::sample_remain(known, &mut rng, n);
            assert_eq!(sampled.len(), n);
            assert!(sampled.is_disjoint(known));
        }
    }
}
