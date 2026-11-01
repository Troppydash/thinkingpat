use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use serde::{Deserialize, Serialize};

use crate::{
    game::{
        board::Street,
        card::{Card, CardSet},
        showdown,
    },
    group::{
        group::Group,
        index::{SuitIndex, choose, choose_usize},
    },
    ver::{self, Versioned},
    versions,
};

const PAIRS: usize = Card::COUNT * (Card::COUNT - 1) / 2;

/// There are C(45, 2) samples, we use 2 as win and 1 as draw so x2. This also fits in u16
const EQUITY_DENOMINATOR: usize = choose_usize(45, 2) * 2;
const MISSING: u16 = u16::MAX;

fn pair_index(first: usize, second: usize) -> usize {
    second * (second - 1) / 2 + first
}

/// Calls f(hand, hand_index) for all hands in [remain]
fn iterate_opp_hands(remain: CardSet, mut f: impl FnMut(CardSet, usize)) {
    CardSet::iterate_community(remain, 2, |hand| {
        let pairs = hand.take::<2>();
        f(hand, pair_index(pairs[0].index(), pairs[1].index()));
    });
}

#[derive(Serialize, Deserialize)]
pub struct River {
    /// (hand, texture) index -> equity
    values: Vec<u16>,
    /// for caching only, texture index -> pair -> equity
    river_trick: Vec<Vec<u16>>,
}

versions!(TextureRiverVersions, V1);

impl Versioned<TextureRiverVersions> for River {
    /// Estimated to take C(52, 5) * C(47, 2) = 2.8e9
    fn build(version: &TextureRiverVersions, _path: &str) -> Self {
        assert!(*version == TextureRiverVersions::V1);

        let community_cards = Street::River.visible_community();
        let full_index = SuitIndex::new(2, community_cards);
        let texture_index = SuitIndex::new(0, community_cards);

        let mut values = vec![MISSING; full_index.size()];
        let mut river_trick = vec![vec![0; PAIRS]; texture_index.size()];
        for i in 0..texture_index.size() {
            let (_empty_hand, texture) = texture_index.unmap(i);

            // we do a neat trick of:
            // 1. compute forall hand in evaluate(hand, texture)
            // 2. sort by eval
            // 3. two pointers

            // list of (eval, weaker, stronger) sorted by eval
            let mut hands = Vec::with_capacity(choose_usize(52 - community_cards, 2));
            iterate_opp_hands(!texture, |hand, _| {
                let pairs = hand.take::<2>();
                hands.push((showdown::evaluate(hand | texture), pairs[0], pairs[1]));
            });
            hands.sort_unstable_by_key(|h| h.0);

            // equities[our hand]
            let equities = &mut river_trick[i];
            let mut weaker_by_card = [0usize; Card::COUNT];
            let mut start = 0;
            while start < hands.len() {
                // start..end has the same eval
                let mut end = start + 1;
                while end < hands.len() && hands[end].0 == hands[start].0 {
                    end += 1;
                }

                // we need to adjust for opp hands exclusive of our hand
                // ties_by_card[card] = #opp_hands tied to this eval with card
                // weaker_by_card[card] = #opp_hands worse than you with card

                let tied_hands = &hands[start..end];
                let mut ties_by_card = [0usize; Card::COUNT];
                for &(_, first, second) in tied_hands {
                    ties_by_card[first.index()] += 1;
                    ties_by_card[second.index()] += 1;
                }

                for &(_, first, second) in tied_hands {
                    let wins =
                        start - weaker_by_card[first.index()] - weaker_by_card[second.index()];

                    // this subs this exact hand twice so +1
                    let ties = tied_hands.len() + 1
                        - ties_by_card[first.index()]
                        - ties_by_card[second.index()];

                    equities[pair_index(first.index(), second.index())] = (2 * wins + ties) as u16;
                }

                for &(_, first, second) in tied_hands {
                    weaker_by_card[first.index()] += 1;
                    weaker_by_card[second.index()] += 1;
                }

                start = end;
            }

            iterate_opp_hands(!texture, |hand, pair| {
                values[full_index.map(hand, texture)] = equities[pair];
            });
        }

        Self {
            values,
            river_trick,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).unwrap()
    }
}

impl River {
    pub fn equity(&self, hand: CardSet, community: CardSet, index: &SuitIndex) -> f32 {
        self.values[index.map(hand, community)] as f32 / EQUITY_DENOMINATOR as f32
    }
}

pub const BINS: usize = 8;
fn equity_bin(equity: u16) -> usize {
    (equity as usize * BINS / EQUITY_DENOMINATOR).min(BINS - 1)
}

#[derive(Serialize, Deserialize)]
pub struct Turn {
    /// (hand, texture) index -> binned equity
    values: Vec<[u8; BINS]>,
}

versions!(TextureTurnVersions, V1);

impl Versioned<TextureTurnVersions> for Turn {
    /// Estimated C(52, 4) * C(48, 1) * C(47, 2) = 14e9
    fn build(version: &TextureTurnVersions, path: &str) -> Self {
        assert!(*version == TextureTurnVersions::V1);

        let river: River = ver::try_load(&TextureRiverVersions::V1, path);

        let community_cards = Street::Turn.visible_community();
        let full_index = SuitIndex::new(2, community_cards);
        let texture_index = SuitIndex::new(0, community_cards);

        let river_index = SuitIndex::new(0, Street::River.visible_community());

        let mut values = vec![[0u8; BINS]; full_index.size()];
        for i in 0..texture_index.size() {
            let (_empty_hand, texture) = texture_index.unmap(i);

            // hist[our hand] = equity distribution
            let mut histograms = [[0u8; BINS]; PAIRS];
            CardSet::iterate_community(!texture, 1, |community| {
                let equities =
                    &river.river_trick[river_index.map(CardSet::EMPTY, texture | community)];
                for (hist, equity) in histograms.iter_mut().zip(equities) {
                    if *equity != MISSING {
                        hist[equity_bin(*equity)] += 1;
                    }
                }
            });

            iterate_opp_hands(!texture, |hand, pair| {
                values[full_index.map(hand, texture)] = histograms[pair];
            });
        }

        Self { values }
    }

    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).unwrap()
    }
}

impl Turn {
    pub fn equity(&self, hand: CardSet, community: CardSet, index: &SuitIndex) -> [f32; BINS] {
        const DENOM: u64 = choose(52 - 4, 1);
        let mut iter = self.values[index.map(hand, community)]
            .iter()
            .map(|e| *e as f32 / DENOM as f32);

        std::array::from_fn(|_| iter.next().unwrap())
    }
}

#[derive(Serialize, Deserialize)]
pub struct Flop {
    /// (hand, texture) index -> binned equity
    values: Vec<[u16; BINS]>,
}

versions!(TextureFlopVersions, V1);

impl Versioned<TextureFlopVersions> for Flop {
    /// Estimated C(52, 3) * C(49, 2) * C(47, 2) = 28e9
    fn build(version: &TextureFlopVersions, path: &str) -> Self {
        assert!(*version == TextureFlopVersions::V1);

        let river: River = ver::try_load(&TextureRiverVersions::V1, path);

        let community_cards = Street::Flop.visible_community();
        let full_index = SuitIndex::new(2, community_cards);
        let texture_index = SuitIndex::new(0, community_cards);

        let river_index = SuitIndex::new(0, Street::River.visible_community());

        let mut values = vec![[0u16; BINS]; full_index.size()];
        for i in 0..texture_index.size() {
            let (_empty_hand, texture) = texture_index.unmap(i);

            // hist[our hand] = equity distribution
            let mut histograms = [[0u16; BINS]; PAIRS];
            CardSet::iterate_community(!texture, 2, |community| {
                let equities =
                    &river.river_trick[river_index.map(CardSet::EMPTY, texture | community)];
                for (hist, equity) in histograms.iter_mut().zip(equities) {
                    if *equity != MISSING {
                        hist[equity_bin(*equity)] += 1;
                    }
                }
            });

            iterate_opp_hands(!texture, |hand, pair| {
                values[full_index.map(hand, texture)] = histograms[pair];
            });
        }

        Self { values }
    }

    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).unwrap()
    }
}

impl Flop {
    pub fn equity(&self, hand: CardSet, community: CardSet, index: &SuitIndex) -> [f32; BINS] {
        const DENOM: u64 = choose(52 - 3, 2);
        let mut iter = self.values[index.map(hand, community)]
            .iter()
            .map(|e| *e as f32 / DENOM as f32);

        std::array::from_fn(|_| iter.next().unwrap())
    }
}
