use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};

use crate::{
    bits,
    game::{
        board::{Player, Street},
        card::CardSet,
        showdown,
    },
    group::{
        group::Group,
        vectordb::{EquityVector, U, Vectordb},
    },
};

/// Visits each size-k subset of remain exactly once, in ascending bit order.
fn iterate_community<F>(remain: CardSet, k: usize, mut f: F)
where
    F: FnMut(CardSet),
{
    // edge cases
    let n = remain.len();
    if k > n {
        return;
    }
    if k == 0 {
        f(CardSet::EMPTY);
        return;
    }

    let mut mask = u64::MAX >> (64 - k);
    let end_mask = mask << (n - k);
    loop {
        f(CardSet(bits::pdep(mask, remain.0)));
        if mask == end_mask {
            break;
        }

        // Gosper's hack advances combinations in the dense n-bit space.
        let c = mask & mask.wrapping_neg();
        let r = mask.wrapping_add(c);
        mask = r | (((r ^ mask) >> 2) / c);
    }
}

fn to_cdf(pdf: Vec<f32>) -> Vec<f32> {
    let mut cumul = 0.0;
    let mut out = vec![0.0; pdf.len()];
    for (i, x) in pdf.iter().enumerate() {
        cumul += x;
        out[i] = cumul;
    }

    out
}

const SIMULATE_HOLE_ITERS: usize = 100;
const SIMULATE_COMMUNITY_ITERS: usize = 100;

fn simulate_equity(
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

    // hole iteration
    for _ in 0..hole_iters {
        let opp_hand = CardSet::sample_remain(hand | community, &mut rng, 2);

        // iterate over all community
        let remain = !(hand | opp_hand | community);
        let k = 5 - community.len();

        let equity = if k == 0 {
            match showdown::winner(hand, opp_hand, community) {
                Some(Player::BB) => 1.0,
                Some(Player::SB) => 0.0,
                None => 0.5,
            }
        } else {
            let mut equity = 0.0;
            let mut count = 0;

            if let Some(community_iters) = community_iters {
                // sample community instead
                for _ in 0..community_iters {
                    let extra_community =
                        CardSet::sample_remain(hand | opp_hand | community, &mut rng, k);
                    equity += match showdown::winner(hand, opp_hand, community | extra_community) {
                        Some(Player::BB) => 1.0,
                        Some(Player::SB) => 0.0,
                        None => 0.5,
                    };
                    count += 1;
                }
            } else {
                iterate_community(remain, k, |extra_community| {
                    equity += match showdown::winner(hand, opp_hand, community | extra_community) {
                        Some(Player::BB) => 1.0,
                        Some(Player::SB) => 0.0,
                        None => 0.5,
                    };
                    count += 1;
                });
            }

            equity / count as f32
        };

        let bin = ((equity / step).floor() as usize).min(bins - 1);
        equity_bins[bin] += 1.0 / hole_iters as f32;
    }

    to_cdf(equity_bins)
}

struct EquityPoint {
    hand: CardSet,
    community: CardSet,
    equity: Vec<f32>,
}

/// hand equity distribution k means sample indexing
pub struct Equity {
    db: Box<dyn Vectordb>,
    // Technically this duplicates data, use an option if not needed
    centroids: Vec<EquityVector>,
    street: Street,
    bins: usize,
}

impl Equity {
    pub fn compute<R: Rng>(
        samples: usize,
        num_centroids: usize,
        bins: usize,
        street: Street,
        rng: &mut R,
    ) -> Self {
        let kmeans_iters = 100;

        // sample points
        let mut points: Vec<EquityPoint> = vec![];
        points.reserve_exact(samples);
        for i in 0..samples {
            if i % 100 == 0 {
                println!("debug samples {}", i);
            }

            let hand = CardSet::sample(rng, 2);
            let community = CardSet::sample_remain(hand, rng, street.visible_community());
            let equity = simulate_equity(
                hand,
                community,
                bins,
                SIMULATE_HOLE_ITERS,
                Some(SIMULATE_COMMUNITY_ITERS),
            );
            let point = EquityPoint {
                hand,
                community,
                equity,
            };
            points.push(point);
        }

        let mut centroids: Vec<EquityVector> =
            rand::seq::index::sample(rng, samples, num_centroids)
                .into_iter()
                .map(|i| points[i].equity.clone())
                .collect();

        let mut db = U::new(bins);

        // k means iteration
        for iter in 0..kmeans_iters {
            if iter % 10 == 0 {
                println!("debug kmeans iter {}", iter);
            }

            db.reset();
            db.reserve(num_centroids);
            for (i, centroid) in centroids.iter().enumerate() {
                db.store(i as u64, centroid);
            }

            // find closest centroid per point
            let mut new_centroids: Vec<Vec<usize>> = vec![vec![]; num_centroids];
            for (i, point) in points.iter().enumerate() {
                let (centroid, _distance) = db.query(&point.equity);
                new_centroids[centroid as usize].push(i);
            }

            // each centroid = average of points
            for (centroid, close_points) in new_centroids.iter().enumerate() {
                if close_points.is_empty() {
                    // this can be bad
                    continue;
                }

                let centroid = &mut centroids[centroid];
                centroid.fill(0.0);
                for p in close_points {
                    for k in 0..bins {
                        centroid[k] += points[*p].equity[k] / close_points.len() as f32;
                    }
                }
            }
        }

        db.reset();
        db.reserve(num_centroids);
        for (i, centroid) in centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        Self {
            db: Box::new(db),
            centroids,
            street,
            bins,
        }
    }

    pub fn save(&self, filename: &str) {
        let data = EquityData {
            centroids: self.centroids.clone(),
            bins: self.bins,
            street: self.street,
        };
        data.save(filename);
    }

    pub fn load(filename: &str) -> Self {
        let data = EquityData::load(filename);

        let mut db = Box::new(U::new(data.bins));
        for (i, centroid) in data.centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        Self {
            db,
            centroids: data.centroids,
            bins: data.bins,
            street: data.street,
        }
    }
}

impl Group for Equity {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        let equity = simulate_equity(
            hand,
            community,
            self.bins,
            SIMULATE_HOLE_ITERS,
            Some(SIMULATE_COMMUNITY_ITERS),
        );
        let (i, _distance) = self.db.query(&equity);

        i as usize
    }

    fn size(&self) -> usize {
        self.centroids.len()
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct EquityData {
    centroids: Vec<EquityVector>,
    street: Street,
    bins: usize,
}

impl EquityData {
    fn save(&self, filename: &str) {
        let bytes: Vec<u8> = postcard::to_allocvec(self).unwrap();
        std::fs::write(filename, bytes).unwrap();
    }

    fn load(filename: &str) -> Self {
        let bytes = std::fs::read(filename).unwrap();
        postcard::from_bytes(&bytes).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_combinations_match_all_subsets() {
        // Exhaust all eight-card availability patterns, including gaps across
        // byte boundaries and high card positions.
        for pattern in 0u64..256 {
            let remain = (0..8).fold(0u64, |bits, i| bits | (((pattern >> i) & 1) << (i * 7)));
            for k in 0..=9 {
                let mut expected = Vec::new();
                let mut subset = remain;
                loop {
                    if subset.count_ones() as usize == k {
                        expected.push(subset);
                    }
                    if subset == 0 {
                        break;
                    }
                    subset = (subset - 1) & remain;
                }
                expected.reverse();

                let mut actual = Vec::new();
                iterate_community(CardSet(remain), k, |cards| actual.push(cards.0));
                assert_eq!(actual, expected, "remain={remain:#x}, k={k}");
            }
        }
    }

    #[test]
    fn full_deck_pairs_and_boundary_sizes() {
        let mut count = 0;
        iterate_community(CardSet::FULL, 2, |cards| {
            assert_eq!(cards.len(), 2);
            assert_eq!(cards.0 & !CardSet::FULL.0, 0);
            count += 1;
        });
        assert_eq!(count, 52 * 51 / 2);

        for k in [0, 52, 53] {
            let mut actual = Vec::new();
            iterate_community(CardSet::FULL, k, |cards| actual.push(cards));
            let expected = match k {
                0 => vec![CardSet::EMPTY],
                52 => vec![CardSet::FULL],
                _ => vec![],
            };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn river_simulate_equity() {
        let mut rng = rand::rng();
        let hand = CardSet::parse("Ah Ad");
        let community = CardSet::parse("4h 6s Js 7c Kh");
        let bins = 10;
        let iters = 200;
        let output = simulate_equity(hand, community, bins, iters, None);
        // assert_eq!(output, vec![0.0; 10]);
        assert!(output[0] < output[9]);
    }

    #[test]
    fn turn_simulate_equity() {
        let mut rng = rand::rng();
        let hand = CardSet::parse("Ah Ad");
        let community = CardSet::parse("4h 6s Js 7c");
        let bins = 10;
        let iters = 200;
        let output = simulate_equity(hand, community, bins, iters, None);
        // assert_eq!(output, vec![0.0; 10]);
        assert!(output[0] < output[9]);
    }
}
