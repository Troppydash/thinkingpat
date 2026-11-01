use fxhash::FxHashMap;

use crate::{
    game::{board::Street, card::CardSet},
    group::{group::Group, index::SuitIndex},
};

pub struct Cache {
    /// mapping from (hand, community) -> group
    groups: [Vec<usize>; Street::COUNT],
    suit_indexes: [Option<SuitIndex>; Street::COUNT],
}

impl Cache {
    pub fn new(street_groups: &[Box<dyn Group>; Street::COUNT]) -> Self {
        let mut groups = [const { vec![] }; Street::COUNT];
        let mut suit_indexes = [const { None }; Street::COUNT];
        for street in Street::ALL {
            if street == Street::River {
                // ignore because too large
                continue;
            }

            println!("info cache for {}", street.to_string());
            // iterate all hand/community
            let suit_index = SuitIndex::new(2, street.visible_community());
            groups[street as usize].reserve_exact(suit_index.size());
            for i in 0..suit_index.size() {
                let (hand, community) = suit_index.unmap(i);
                let group = street_groups[street as usize].map(hand, community);
                groups[street as usize].push(group);
            }

            assert!(groups[street as usize].len() == suit_index.size());

            suit_indexes[street as usize] = Some(suit_index);
        }

        println!("info cache done");
        Self {
            groups,
            suit_indexes,
        }
    }

    pub fn lookup_group(&self, hand: CardSet, community: CardSet, street: Street) -> usize {
        let index = self.suit_indexes[street as usize]
            .as_ref()
            .unwrap()
            .map(hand, community);
        self.groups[street as usize][index]
    }
}
