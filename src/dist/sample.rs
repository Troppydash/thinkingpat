use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};

use crate::{
    game::{
        card::{Card, CardSet},
        showdown,
    },
    group::{
        group::Group,
        index::{SuitIndex, choose_usize},
    },
    ver::Versioned,
    versions,
};

const SIMULATE_HOLE_ITERS: usize = 100;
const SIMULATE_COMMUNITY_ITERS: usize = 100;

pub fn simulate_equity(
    hand: CardSet,
    community: CardSet,
    bins: usize,
    hole_iters: usize,
    community_iters: Option<usize>,
) -> Vec<f32> {
    // we want a deterministic rng
    let mut seed_bytes = [0u8; 32];
    seed_bytes[..8].copy_from_slice(&hand.0.to_le_bytes());
    seed_bytes[8..16].copy_from_slice(&community.0.to_le_bytes());
    let mut rng = StdRng::from_seed(seed_bytes);

    // iterate all possible remaining community
    // rough counts: river -> 1, turn -> 44, flop -> 990

    // note that we use vecs here because slowdown should be in the simulation step
    let mut equity_bins = vec![0.0; bins];
    let step = 1.0 / bins as f32;
    let k = 5 - community.len();

    let cache_hero = community.len() >= 3;
    let mut hero_ranks = [None; Card::COUNT * (Card::COUNT - 1) / 2];
    let mut showdown_equity = |opp_hand: CardSet, full_community: CardSet| {
        let hero_rank = if cache_hero {
            let extra = (full_community - community).0;
            // Dense index for an empty runout, one card, or an unordered pair.
            let index = match k {
                0 => 0,
                1 => extra.trailing_zeros() as usize,
                2 => {
                    let high = extra.ilog2() as usize;
                    high * (high - 1) / 2 + extra.trailing_zeros() as usize
                }
                _ => unreachable!(),
            };
            *hero_ranks[index].get_or_insert_with(|| showdown::evaluate(hand | full_community))
        } else {
            showdown::evaluate(hand | full_community)
        };
        match hero_rank.cmp(&showdown::evaluate(opp_hand | full_community)) {
            std::cmp::Ordering::Greater => 1.0,
            std::cmp::Ordering::Less => 0.0,
            std::cmp::Ordering::Equal => 0.5,
        }
    };

    // hole iteration
    for _ in 0..hole_iters {
        let opp_hand = CardSet::sample_remain(hand | community, &mut rng, 2);

        // iterate over all community
        let remain = !(hand | opp_hand | community);

        let equity = if k == 0 {
            showdown_equity(opp_hand, community)
        } else {
            let mut equity = 0.0;
            let mut count = 0;

            let full = choose_usize(remain.len(), k);
            if let Some(community_iters) = community_iters
                && community_iters < full
            {
                // sample community instead
                for _ in 0..community_iters {
                    let extra_community =
                        CardSet::sample_remain(hand | opp_hand | community, &mut rng, k);
                    equity += showdown_equity(opp_hand, community | extra_community);
                    count += 1;
                }
            } else {
                CardSet::iterate_community(remain, k, |extra_community| {
                    equity += showdown_equity(opp_hand, community | extra_community);
                    count += 1;
                });
            }

            equity / count as f32
        };

        let bin = ((equity / step).floor() as usize).min(bins - 1);
        equity_bins[bin] += 1.0 / hole_iters as f32;
    }

    equity_bins
}

pub const BINS: usize = 10;

#[derive(Serialize, Deserialize)]
pub struct Preflop {
    /// mapping from hand -> equity bins
    values: Vec<[f32; BINS]>,
}

// V1: 10 bins, 100 hole, 100 community
versions!(SamplePreflopVersions, V1);

impl Versioned<SamplePreflopVersions> for Preflop {
    fn build(version: &SamplePreflopVersions, path: &str) -> Self {
        assert!(*version == SamplePreflopVersions::V1);

        let hand_index = SuitIndex::new(2, 0);

        let mut values = vec![[0.; BINS]; hand_index.size()];
        for i in 0..hand_index.size() {
            let (hand, _empty_community) = hand_index.unmap(i);
            let bins = simulate_equity(
                hand,
                CardSet::EMPTY,
                BINS,
                SIMULATE_HOLE_ITERS,
                Some(SIMULATE_COMMUNITY_ITERS),
            );

            let mut iter = bins.into_iter();
            values[i] = std::array::from_fn(|_| iter.next().unwrap());
        }

        Self { values }
    }

    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).unwrap()
    }

    fn from_bytes(_version: &SamplePreflopVersions, _path: &str, bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).unwrap()
    }
}

impl Preflop {
    pub fn equity(&self, hand: CardSet, index: &SuitIndex) -> [f32; BINS] {
        self.values[index.map(hand, CardSet::EMPTY)]
    }
}
