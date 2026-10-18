use crate::{game::card::Card, group::group::Group};
/// hand equity distribution k means sample indexing

fn simulate_equity(hand: &[Card], community: &[Card], iters: usize) {
    // iterate all possible remaining community
    // rough counts: river -> 1, turn -> 44, flop -> 990
    todo!()
}

struct Equity {}
impl Equity {}

impl Group for Equity {
    fn map(&self, cards: &[Card]) -> usize {
        todo!()
    }

    fn size(&self) -> usize {
        todo!()
    }
}
