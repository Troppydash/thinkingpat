use fxhash::FxHashMap;

use crate::{
    game::{board::Street, card::CardSet},
    group::group::Group,
};

pub struct Cache {
    pub hand_groups: FxHashMap<CardSet, usize>,
}

impl Cache {
    pub fn new(hand_group: &Box<dyn Group>) -> Self {
        let mut hand_groups: FxHashMap<CardSet, usize> = FxHashMap::default();
        CardSet::iterate_community(CardSet::FULL, 2, |hand| {
            hand_groups.insert(hand, hand_group.map(hand, CardSet::EMPTY));
        });

        Self { hand_groups }
    }
}
