use std::cmp::Ordering;

use super::board::Player;
use super::card::{Card, Rank};

pub fn winner(bb: &[Card], sb: &[Card], community: &[Card]) -> Option<Player> {
    match evaluate(bb, community).cmp(&evaluate(sb, community)) {
        Ordering::Greater => Some(Player::BB),
        Ordering::Less => Some(Player::SB),
        Ordering::Equal => None,
    }
}

const STRAIGHT_FLUSH: u8 = 8;
const QUAD: u8 = 7;
const FULL_HOUSE: u8 = 6;
const FLUSH: u8 = 5;
const STRAIGHT: u8 = 4;
const THREE: u8 = 3;
const TWO_PAIR: u8 = 2;
const PAIR: u8 = 1;

// Some(high_card) if straight ending at high, None if no straight
fn straight_high(mask: u16) -> Option<u8> {
    for high in (4..13).rev() {
        let run = 0b11111 << (high - 4);
        if mask & run == run {
            return Some(high);
        }
    }
    if (mask & 0b1_0000_0000_1111) == 0b1_0000_0000_1111 {
        return Some(3);
    }
    None
}

// Fill in value with top slots from mask
fn kickers(mut value: [u8; 6], mut mask: u16, slots: std::ops::Range<usize>) -> [u8; 6] {
    for slot in slots {
        let rank = mask.ilog2() as u8;
        value[slot] = rank;
        mask &= !(1 << rank);
    }
    value
}

// [category, high card, kickers...]
fn evaluate(hole: &[Card], community: &[Card]) -> [u8; 6] {
    let mut counts = [0u8; 13];
    let mut suits = [0u16; 4];
    let mut ranks = 0u16;
    for card in hole.iter().chain(community) {
        let rank = card.rank as usize;
        counts[rank] += 1;
        suits[card.suit as usize] |= 1 << rank;
        ranks |= 1 << rank;
    }

    let flush = suits.into_iter().find(|mask| mask.count_ones() >= 5);
    if let Some(high) = flush.and_then(straight_high) {
        return [STRAIGHT_FLUSH, high, 0, 0, 0, 0];
    }

    let mut quads = None;
    let mut trips = None;
    let mut pairs = [0u8; 3];
    let mut pair_count = 0;
    for rank in (0..13).rev() {
        match counts[rank] {
            4 => quads = Some(rank as u8),
            3 if trips.is_none() => trips = Some(rank as u8),
            2 | 3 => {
                pairs[pair_count] = rank as u8;
                pair_count += 1;
            }
            _ => {}
        }
    }

    if let Some(rank) = quads {
        let kicker = (ranks & !(1 << rank)).ilog2() as u8;
        return [QUAD, rank, kicker, 0, 0, 0];
    }
    if let Some(rank) = trips {
        if pair_count > 0 {
            return [FULL_HOUSE, rank, pairs[0], 0, 0, 0];
        }
    }
    if let Some(mask) = flush {
        return kickers([FLUSH, 0, 0, 0, 0, 0], mask, 1..6);
    }
    if let Some(high) = straight_high(ranks) {
        return [STRAIGHT, high, 0, 0, 0, 0];
    }
    if let Some(rank) = trips {
        return kickers([THREE, rank, 0, 0, 0, 0], ranks & !(1 << rank), 2..4);
    }
    if pair_count >= 2 {
        let mask = ranks & !(1 << pairs[0]) & !(1 << pairs[1]);
        return [TWO_PAIR, pairs[0], pairs[1], mask.ilog2() as u8, 0, 0];
    }
    if pair_count == 1 {
        return kickers([PAIR, pairs[0], 0, 0, 0, 0], ranks & !(1 << pairs[0]), 2..5);
    }
    kickers([0; 6], ranks, 1..6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_every_hand_category() {
        let cases = [
            ("As Kd Qh Jc 9s 5d 2c", [0, 12, 11, 10, 9, 7]),
            ("As Ad Kh Qc Js 5d 2c", [1, 12, 11, 10, 9, 0]),
            ("As Ad Kh Kc Qs 5d 2c", [2, 12, 11, 10, 0, 0]),
            ("As Ad Ah Kc Qs 5d 2c", [3, 12, 11, 10, 0, 0]),
            ("As Kd Qh Jc Ts 5d 2c", [4, 12, 0, 0, 0, 0]),
            ("As Js 9s 6s 4s 3s 2c", [5, 12, 9, 7, 4, 2]),
            ("As Ad Ah Kc Ks 5d 2c", [6, 12, 11, 0, 0, 0]),
            ("As Ad Ah Ac Ks 5d 2c", [7, 12, 11, 0, 0, 0]),
            ("As Ks Qs Js Ts 5d 2c", [8, 12, 0, 0, 0, 0]),
        ];
        let mut previous = None;
        for (text, expected) in cases {
            let cards = Card::parse_cards::<7>(text);
            let value = evaluate(&cards[..2], &cards[2..]);
            assert_eq!(value, expected, "{text}");
            if let Some(previous) = previous {
                assert!(value > previous, "{text}");
            }
            previous = Some(value);
        }
    }

    #[test]
    fn chooses_the_best_five_cards() {
        for (text, expected) in [
            ("As Ad Ah Kc Ks Kd 2c", [6, 12, 11, 0, 0, 0]),
            ("As Ad Kh Kc Qs Qd 2c", [2, 12, 11, 10, 0, 0]),
            ("2s 2d 2h Ac As Kd Kh", [6, 0, 12, 0, 0, 0]),
            ("As 2d 3h 4c 5s 9d Tc", [4, 3, 0, 0, 0, 0]),
            ("As 2d 3h 4c 5s 6d Tc", [4, 4, 0, 0, 0, 0]),
            ("Ah 2h 3h 4h 5h 6h Kc", [8, 4, 0, 0, 0, 0]),
            ("Ah 2h 3h 4h 5h 9d Tc", [8, 3, 0, 0, 0, 0]),
            ("Ah Kh Qh Jh 9h 2h 3h", [5, 12, 11, 10, 9, 7]),
            ("Ah Kh Qh Jh 9h Tc 2d", [5, 12, 11, 10, 9, 7]),
            ("As Ad Kh Qc Js Td 2c", [4, 12, 0, 0, 0, 0]),
            ("As 2d 3h 4c 6s 9d Tc", [0, 12, 8, 7, 4, 2]),
            ("As Kd Qh Jc 2s 5d 3c", [0, 12, 11, 10, 9, 3]),
        ] {
            let cards = Card::parse_cards::<7>(text);
            assert_eq!(evaluate(&cards[..2], &cards[2..]), expected, "{text}");
        }
    }

    #[test]
    fn compares_all_tiebreakers() {
        for text in [
            "Kh Qd Jh Td As 9c 7h 4d 2s",
            "Qh 8d Qc 7d As Ks Qs 4h 2c",
            "Jh 8d Jc 7d As Ks Qs 4h 2c",
            "Th 8d Tc 7d As Ks Qs 4h 2c",
            "Qh Qd Jh Jd As 9c 7h 4d 2s",
            "Kh Kd Qh Qd As Ac 7h 4d 2s",
            "Qh Jd Tc 9d As Ac Ks Kc 2s",
            "Kh Kd Qh Qd Ks Qs 7h 4d 2s",
            "Qh Jd Qc Td As Ac Ah 4d 2s",
            "6h 2d Ah 2c 3s 4c 5h 9d Ks",
            "Kh 2d Qh 3d Ah Jh 8h 5h 4h",
            "9h 2d 8h 3d Ah Kh Qh Jh 4h",
            "Qh Qd Jh Jd As Ac Ah 4d 2s",
            "Ah 2d Ks 2c As Ad Kh Kd 3s",
            "Kh 2d Qh 3d As Ac Ah Ad 4s",
            "3s 3c 2s 2c 2h 2d 3h 3d As",
            "9h 2d 4h 3d 5h 6h 7h 8h Ks",
            "Kh 2d Qh 3d As 9c 7h 4d 2s",
        ] {
            let mut cards = Card::parse_cards::<9>(text);
            assert_eq!(
                winner(&cards[..2], &cards[2..4], &cards[4..]),
                Some(Player::BB),
                "{text}"
            );
            cards.swap(0, 2);
            cards.swap(1, 3);
            assert_eq!(
                winner(&cards[..2], &cards[2..4], &cards[4..]),
                Some(Player::SB),
                "swapped {text}"
            );
        }
    }

    #[test]
    fn ties_ignore_suits_and_unused_cards() {
        for text in [
            "2h 3d 4h 5d As Ks Qs Js Ts",
            "Ah Kd Ad Kh Qs Jc 9h 4d 2s",
            "2h 3d 4h 5d As Ac Ah Ad Ks",
            "Kh 2d Kc 3d As Ac 7h 7d Qs",
            "2h 3d 4h 5d As Ac Ah Ks Kc",
            "Ah 2d Ac 3d Ks Qc Jh Td 9s",
        ] {
            let cards = Card::parse_cards::<9>(text);
            assert_eq!(
                winner(&cards[..2], &cards[2..4], &cards[4..]),
                None,
                "{text}"
            );
        }
    }
}
