use crate::{game::card::CardSet, group::group::Group};
/// hand equity distribution k means sample indexing

fn simulate_equity(hand: CardSet, community: CardSet, iters: usize) {
    // iterate all possible remaining community
    // rough counts: river -> 1, turn -> 44, flop -> 990
    todo!()
}

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
