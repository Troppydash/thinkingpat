use crate::game::card::CardSet;

pub trait Group {
    fn map(&self, cards: CardSet) -> usize;
    fn size(&self) -> usize;
}
