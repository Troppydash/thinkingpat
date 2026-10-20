use crate::game::card::CardSet;

pub trait Group {
    fn map(&self, hand: CardSet, community: CardSet) -> usize;
    fn size(&self) -> usize;
}
