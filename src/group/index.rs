use std::cmp::Reverse;

use fxhash::FxHashMap;

use crate::{
    game::card::{Card, CardSet, Rank, Suit},
    group::group::Group,
};

fn choose(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut out = 1;
    for i in 0..k {
        out = out * (n - i) / (i + 1);
    }
    out
}

// Combinatorial number system for a combination
fn cns(mut mask: u16) -> usize {
    let mut value = 0;
    let mut k = mask.count_ones();
    while mask > 0 {
        let c = mask.ilog2();
        value += choose(c as u64, k as u64) as usize;
        k -= 1;
        mask &= !(1 << c);
    }
    value
}

// Descending (hand count, community count) pairs, one per suit.
type PatternKey = [[u8; 2]; Suit::COUNT];
struct PatternData {
    // Mixed-radix stride at the start of each equal-count suit group.
    strides: [usize; Suit::COUNT],
    offset: usize,
}

// For a specific suit, index (hand, community)
fn index_suit(hand: u16, community: u16) -> usize {
    // note the subset for community always excludes hand, so we are free to exclude them
    let mut compressed = 0;

    #[cfg(target_feature = "bmi2")]
    {
        compressed = std::arch::x86_64::_pext_u64(community, !hand);
    }

    #[cfg(not(target_feature = "bmi2"))]
    {
        let mut i = 0;
        for rank in 0..Rank::COUNT {
            if hand & (1 << rank) == 0 {
                if community & (1 << rank) != 0 {
                    compressed |= 1 << i;
                }
                i += 1;
            }
        }
    }

    cns(hand) + choose(Rank::COUNT as u64, hand.count_ones() as u64) as usize * cns(compressed)
}

pub struct SuitIndex {
    hand_cards: usize,
    community_cards: usize,
    size: usize,
    patterns: FxHashMap<PatternKey, PatternData>,
}

impl SuitIndex {
    pub fn new(hand_cards: usize, community_cards: usize) -> Self {
        // This is stricter than [Card::COUNT] for an optimization in [iterate_pattern_keys]
        // where we don't check for capacity in a later suit
        assert!(hand_cards <= Rank::COUNT && community_cards <= Rank::COUNT - hand_cards);

        fn iterate_pattern_keys(
            // [hand, community]
            remaining: [usize; 2],
            suit_index: usize,
            key: &mut PatternKey,
            f: &mut impl FnMut(PatternKey),
        ) {
            if suit_index == Suit::COUNT {
                if remaining == [0, 0] {
                    f(*key);
                }
                return;
            }

            for hand in 0..=remaining[0] {
                for community in 0..=remaining[1] {
                    // forall combinations of remaining to this suit

                    let counts = [hand as u8, community as u8];
                    if suit_index > 0 && counts > key[suit_index - 1] {
                        continue;
                    }

                    let left = [remaining[0] - hand, remaining[1] - community];
                    key[suit_index] = counts;
                    iterate_pattern_keys(left, suit_index + 1, key, f);
                }
            }
        }

        let mut patterns = FxHashMap::default();
        let mut offset = 0usize;
        iterate_pattern_keys(
            [hand_cards, community_cards],
            0,
            &mut [[0; 2]; Suit::COUNT],
            &mut |key| {
                let mut count = 1usize;
                let mut strides = [0; Suit::COUNT];
                let mut start = 0;
                while start < Suit::COUNT {
                    // Suits with the same counts form a multiset of disjoint hand/community rank combinations.
                    let mut end = start + 1;
                    while end < Suit::COUNT && key[end] == key[start] {
                        end += 1;
                    }
                    strides[start] = count;

                    let [hand, community] = key[start].map(u64::from);
                    let ranks = choose(Rank::COUNT as u64, hand)
                        * choose(Rank::COUNT as u64 - hand, community);
                    let suits = (end - start) as u64;
                    count *= choose(ranks + suits - 1, suits) as usize;
                    start = end;
                }
                patterns.insert(key, PatternData { strides, offset });
                offset += count;
            },
        );
        Self {
            hand_cards,
            community_cards,
            patterns,
            size: offset,
        }
    }
}

impl Group for SuitIndex {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        debug_assert_eq!(
            hand.len(),
            self.hand_cards,
            "incorrect number of cards in hand"
        );
        debug_assert_eq!(
            community.len(),
            self.community_cards,
            "incorrect number of cards in community"
        );
        debug_assert!(hand.is_disjoint(community), "hand and community overlap");

        let mut suits = Suit::ALL.map(|suit| {
            let hand = hand.suit_ranks(suit);
            let community = community.suit_ranks(suit);
            (
                [hand.count_ones() as u8, community.count_ones() as u8],
                index_suit(hand, community),
            )
        });
        suits.sort_unstable_by_key(|&(counts, value)| (Reverse(counts), value));
        let pattern = suits.map(|(counts, _)| counts);
        let data = self
            .patterns
            .get(&pattern)
            .expect("missing suit-count pattern");

        let mut index = 0;
        let mut start = 0;
        for (i, &(_, value)) in suits.iter().enumerate() {
            if i > 0 && pattern[i] != pattern[i - 1] {
                start = i;
            }

            // note that [suits] is sorted ascend so cns is increasing within each multiset
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
        ];
        for (cards, counts) in expected {
            let index = SuitIndex::new(cards, 0);
            assert_eq!(counts, index.size());
        }

        let expected = [(0, 169), (3, 1286792), (4, 13960050), (5, 123156254)];
        for (cards, counts) in expected {
            let index = SuitIndex::new(2, cards);
            assert_eq!(counts, index.size());
        }
    }

    #[test]
    fn exhaustive_small_hands_have_no_collisions_or_gaps() {
        // Canonical masks provide an independent identity for each suit orbit.
        for count in 1..=5 {
            let index = SuitIndex::new(count, 0);
            let mut seen = vec![None; index.size()];
            fn visit(
                index: &SuitIndex,
                hand: CardSet,
                next: usize,
                seen: &mut [Option<[u16; Suit::COUNT]>],
            ) {
                if hand.len() == index.hand_cards {
                    let mut canonical = [0u16; Suit::COUNT];
                    for card in hand {
                        canonical[card.suit() as usize] |= 1 << card.rank() as usize;
                    }
                    canonical.sort_unstable();
                    let slot = &mut seen[index.map(hand, CardSet::EMPTY)];
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
            let index = SuitIndex::new(hand.len(), 0);
            let expected = index.map(hand, CardSet::EMPTY);
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
                            assert_eq!(index.map(mapped, CardSet::EMPTY), expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn empty_decks_map_to_zero() {
        assert_eq!(SuitIndex::new(0, 0).map(CardSet::EMPTY, CardSet::EMPTY), 0);
    }

    #[test]
    fn ownership_and_shared_suits_are_preserved() {
        let index = SuitIndex::new(2, 3);
        let hand = CardSet::parse("Ah Kh");
        let board = CardSet::parse("Qh Jh Th");
        // Same union, different ownership.
        assert_ne!(
            index.map(hand, board),
            index.map(CardSet::parse("Ah Qh"), CardSet::parse("Kh Jh Th")),
        );
        // Independent suit canonicalization would merge these two boards.
        assert_ne!(
            index.map(hand, board),
            index.map(hand, CardSet::parse("Qs Js Ts"))
        );
    }

    #[test]
    fn shared_suit_permutations_preserve_partitioned_index() {
        for board in ["2h 3d 4s", "2h 3d 4s 5c", "2h 3d 4s 5c 6h"] {
            let hand = CardSet::parse("Ah Kd");
            let board = CardSet::parse(board);
            let index = SuitIndex::new(hand.len(), board.len());
            let expected = index.map(hand, board);
            assert!(expected < index.size());
            for a in 0..4 {
                for b in 0..4 {
                    for c in 0..4 {
                        for d in 0..4 {
                            let permutation = [a, b, c, d];
                            if (0..4).any(|i| (0..i).any(|j| permutation[i] == permutation[j])) {
                                continue;
                            }
                            let permute = |cards: CardSet| {
                                cards
                                    .iter()
                                    .rev()
                                    .map(|card| {
                                        Card::new(
                                            Suit::ALL[permutation[card.suit() as usize]],
                                            card.rank(),
                                        )
                                    })
                                    .collect()
                            };
                            assert_eq!(index.map(permute(hand), permute(board)), expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn exhaustive_small_partitions_have_no_collisions_or_gaps() {
        for (hand_count, community_count) in [(0, 2), (1, 1), (1, 2), (2, 1)] {
            let index = SuitIndex::new(hand_count, community_count);
            let mut seen = vec![None; index.size()];
            fn visit(
                index: &SuitIndex,
                hand: CardSet,
                community: CardSet,
                next: usize,
                seen: &mut [Option<[[u16; 2]; Suit::COUNT]>],
            ) {
                let filling_hand = hand.len() < index.hand_cards;
                if !filling_hand && community.len() == index.community_cards {
                    let mut canonical =
                        Suit::ALL.map(|suit| [hand.suit_ranks(suit), community.suit_ranks(suit)]);
                    canonical.sort_unstable();
                    let slot = &mut seen[index.map(hand, community)];
                    if let Some(previous) = slot {
                        assert_eq!(*previous, canonical, "collision: {hand:?}, {community:?}");
                    } else {
                        *slot = Some(canonical);
                    }
                    return;
                }
                for card_index in next..Card::COUNT {
                    let card = CardSet::from(Card::ALL[card_index]);
                    if filling_hand {
                        let hand = hand | card;
                        let next = if hand.len() == index.hand_cards {
                            0
                        } else {
                            card_index + 1
                        };
                        visit(index, hand, community, next, seen);
                    } else if hand.is_disjoint(card) {
                        visit(index, hand, community | card, card_index + 1, seen);
                    }
                }
            }
            visit(&index, CardSet::EMPTY, CardSet::EMPTY, 0, &mut seen);
            assert!(
                seen.iter().all(Option::is_some),
                "gaps for ({hand_count}, {community_count})"
            );
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "incorrect number of cards in community")]
    fn rejects_wrong_community_size() {
        SuitIndex::new(2, 3).map(CardSet::parse("Ah Kh"), CardSet::parse("Qh Jh"));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "hand and community overlap")]
    fn rejects_overlapping_partitions() {
        SuitIndex::new(2, 3).map(CardSet::parse("Ah Kh"), CardSet::parse("Ah Qh Jh"));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "incorrect number of cards")]
    fn rejects_wrong_hand_size() {
        SuitIndex::new(2, 0).map(CardSet::from(Card::ALL[0]), CardSet::EMPTY);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "incorrect number of cards")]
    fn duplicate_cards_do_not_increase_hand_size() {
        SuitIndex::new(2, 0).map(CardSet::from([Card::ALL[0]; 2]), CardSet::EMPTY);
    }
}
