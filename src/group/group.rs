use std::hash::Hash;

use crate::game::card::Card;

pub trait Group {
    fn map(&self, cards: &[Card]) -> usize;
    fn size(&self) -> usize;
}