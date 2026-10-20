use std::cmp::Reverse;

use fxhash::FxHashMap;

use crate::{
    game::card::{Card, CardSet, Rank, Suit},
    group::group::Group,
};

// number of cards in each of suit, must be in descending order
type PatternKey = [u8; 4];
struct PatternData {
    // mixed-radix stride at the start of each equal-count suit group.
    strides: [usize; Suit::COUNT],
    // offset is the arbitrary offset to index this pattern
    offset: usize,
}

fn choose(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }

    // nCk = nC(n-k)
    let k = k.min(n - k);
    let mut out = 1;
    for i in 0..k {
        out = out * (n - i) / (i + 1);
    }

    out
}

pub struct SuitIndex {
    cards: usize,
    size: usize,
    patterns: FxHashMap<PatternKey, PatternData>,
}

impl SuitIndex {
    pub fn new(cards: usize) -> Self {
        assert!(cards <= Card::COUNT);
        let mut patterns = FxHashMap::default();

        let mut offset = 0;
        fn iterate_pattern_keys<F>(cards: usize, mut f: F)
        where
            F: FnMut(PatternKey),
        {
            // iterate instead of recurse over 4 suits
            for a in 0..=(Rank::COUNT as u8) {
                for b in 0..=a {
                    for c in 0..=b {
                        if a + b + c > cards as u8 {
                            continue;
                        }

                        let d = cards as u8 - (a + b + c);
                        if d > c {
                            continue;
                        }

                        f([a, b, c, d]);
                    }
                }
            }
        }
        iterate_pattern_keys(cards, |key| {
            let mut count = 1;
            let mut strides = [0; Suit::COUNT];
            let mut start = 0;
            while start < Suit::COUNT {
                // equal-count suits form a multiset of rank combinations.

                let mut end = start + 1;
                while end < Suit::COUNT && key[end] == key[start] {
                    end += 1;
                }

                strides[start] = count;
                let ranks = choose(Rank::COUNT as u64, key[start] as u64);
                let suits = (end - start) as u64;
                count *= choose(ranks + suits - 1, suits) as usize;
                start = end;
            }
            patterns.insert(key, PatternData { strides, offset });
            offset += count;
        });

        Self {
            cards,
            patterns,
            size: offset,
        }
    }
}

impl Group for SuitIndex {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        // TODO: uh hmm



        
        debug_assert_eq!(cards.len(), self.cards, "incorrect number of cards");
        // group by suit sort by count
        let mut suit = Suit::ALL.map(|suit| cards.suit_ranks(suit));

        suit.sort_unstable_by_key(|mask| (Reverse(mask.count_ones()), *mask));
        let pattern = suit.map(|mask| mask.count_ones() as u8);
        let data = self
            .patterns
            .get(&pattern)
            .expect("missing suit-count pattern");

        let mut index = 0;
        let mut start = 0;
        for (i, mask) in suit.iter().enumerate() {
            if i > 0 && pattern[i] != pattern[i - 1] {
                start = i;
            }

            // cns
            let mut value = 0;
            let mut mask = *mask;
            let mut k = mask.count_ones();
            while mask > 0 {
                let c = mask.ilog2();
                value += choose(c as u64, k as u64) as usize;
                k -= 1;
                mask &= !(1 << c);
            }

            // note that [mask] is sorted ascend so cns is increasing within each multiset
            // this multiset is over (cns0 + 0, cns1 + 1, ...)
            let position = (i - start) as u64;
            index += choose(value as u64 + position, position + 1) as usize * data.strides[start];
        }

        index + data.offset
    }

    fn size(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suit_index_size_reduction() {
        let expected = [
            (0, 1),
            (1, 13),
            (2, 169),
            (3, 1755),
            (4, 16432),
            (5, 134459),
            (6, 962988),
            (7, 6009159),
            (52, 1),
        ];
        for (cards, counts) in expected {
            let index = SuitIndex::new(cards);
            assert_eq!(counts, index.size());
        }
    }

    #[test]
    fn exhaustive_small_hands_have_no_collisions_or_gaps() {
        // Canonical masks provide an independent identity for each suit orbit.
        for count in 1..=5 {
            let index = SuitIndex::new(count);
            let mut seen = vec![None; index.size()];
            fn visit(
                index: &SuitIndex,
                hand: CardSet,
                next: usize,
                seen: &mut [Option<[u16; Suit::COUNT]>],
            ) {
                if hand.len() == index.cards {
                    let mut canonical = [0u16; Suit::COUNT];
                    for card in hand {
                        canonical[card.suit() as usize] |= 1 << card.rank() as usize;
                    }
                    canonical.sort_unstable();
                    let slot = &mut seen[index.map(hand)];
                    if let Some(previous) = slot {
                        assert_eq!(*previous, canonical, "index collision: {hand:?}");
                    } else {
                        *slot = Some(canonical);
                    }
                    return;
                }
                for card in next..Card::COUNT {
                    visit(index, hand | CardSet::from(Card::ALL[card]), card + 1, seen);
                }
            }
            visit(&index, CardSet::EMPTY, 0, &mut seen);
            assert!(
                seen.iter().all(Option::is_some),
                "unused indices for {count} cards"
            );
        }
    }

    #[test]
    fn suit_permutations_and_card_order_preserve_index() {
        for text in ["2h 3h 2d 3d As", "Ah Kh Qh Jh Th", "2h 3d 4s 5c 6h 7d 8s"] {
            let hand = CardSet::parse(text);
            let index = SuitIndex::new(hand.len());
            let expected = index.map(hand);
            assert!(expected < index.size());
            for a in 0..4 {
                for b in 0..4 {
                    for c in 0..4 {
                        for d in 0..4 {
                            let permutation = [a, b, c, d];
                            if (0..4).any(|i| (0..i).any(|j| permutation[i] == permutation[j])) {
                                continue;
                            }
                            let mapped: CardSet = hand
                                .iter()
                                .rev()
                                .map(|card| {
                                    Card::new(
                                        Suit::ALL[permutation[card.suit() as usize]],
                                        card.rank(),
                                    )
                                })
                                .collect();
                            assert_eq!(index.map(mapped), expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn empty_and_full_decks_map_to_zero() {
        assert_eq!(SuitIndex::new(0).map(CardSet::EMPTY), 0);
        assert_eq!(SuitIndex::new(Card::COUNT).map(CardSet::FULL), 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "incorrect number of cards")]
    fn rejects_wrong_hand_size() {
        SuitIndex::new(2).map(CardSet::from(Card::ALL[0]));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "incorrect number of cards")]
    fn duplicate_cards_do_not_increase_hand_size() {
        SuitIndex::new(2).map(CardSet::from([Card::ALL[0]; 2]));
    }
}
