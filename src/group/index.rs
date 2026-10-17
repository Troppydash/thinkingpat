// lossless indexing

use crate::group::group::Group;

pub struct SuitIndex {
    cards: usize,
}

impl SuitIndex {
    pub fn new(cards: usize) -> Self {
        Self { cards }
    }
}

impl Group for SuitIndex {
    fn map(&self, cards: &[crate::game::card::Card]) -> usize {
        // group by suit sort by count

        // hmm total count is
        todo!()
    }
    
    fn size(&self) -> usize {
        todo!()
    }
}
