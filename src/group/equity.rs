use rand::Rng;

use crate::{
    game::{board::Player, card::CardSet, showdown},
    group::group::Group,
};

/// Maps dense bit positions onto the available card positions.
#[inline]
fn deposit_bits(source: u64, available: u64) -> u64 {
    #[cfg(target_feature = "bmi2")]
    {
        unsafe { core::arch::x86_64::_pdep_u64(source, available) }
    }

    #[cfg(not(target_feature = "bmi2"))]
    {
        let mut source = source;
        let mut available = available;
        let mut out = 0;
        while source != 0 {
            let lowest = available.isolate_lowest_one();
            if source & 1 != 0 {
                out |= lowest;
            }
            source >>= 1;
            available ^= lowest;
        }
        out
    }
}

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
        f(CardSet(deposit_bits(mask, remain.0)));
        if mask == end_mask {
            break;
        }

        // Gosper's hack advances combinations in the dense n-bit space.
        let c = mask & mask.wrapping_neg();
        let r = mask.wrapping_add(c);
        mask = r | (((r ^ mask) >> 2) / c);
    }
}

fn simulate_equity<R: Rng>(
    rng: &mut R,
    hand: CardSet,
    community: CardSet,
    bins: usize,
    iters: usize,
) -> Vec<f32> {
    // iterate all possible remaining community
    // rough counts: river -> 1, turn -> 44, flop -> 990

    // note that we use vecs here because slowdown should be in the simulation step
    let mut equity_bins = vec![0.0; bins];
    let step = 1.0 / bins as f32;

    // hole iteration
    let mut i = 0;
    while i < iters {
        let opp_hand = CardSet::sample_remain(hand | community, rng, 2);

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
            iterate_community(remain, k, |extra_community| {
                equity += match showdown::winner(hand, opp_hand, community | extra_community) {
                    Some(Player::BB) => 1.0,
                    Some(Player::SB) => 0.0,
                    None => 0.5,
                };
                count += 1;
            });

            equity / count as f32
        };

        let bin = ((equity / step).floor() as usize).min(bins - 1);
        equity_bins[bin] += 1.0 / iters as f32;

        i += 1;
    }

    equity_bins
}

/// hand equity distribution k means sample indexing
struct Equity {}
impl Equity {}

impl Group for Equity {
    fn map(&self, cards: CardSet) -> usize {
        todo!()
    }

    fn size(&self) -> usize {
        todo!()
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
        let output = simulate_equity(&mut rng, hand, community, bins, iters);
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
        let output = simulate_equity(&mut rng, hand, community, bins, iters);
        // assert_eq!(output, vec![0.0; 10]);
        assert!(output[0] < output[9]);
    }
}
