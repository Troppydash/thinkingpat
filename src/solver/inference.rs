use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
};

use serde::{Deserialize, Serialize};

use crate::{
    game::board::{Action, Board, Street},
    group::group::Group,
    solver::engine::{Actions, EngineGroups, MAX_ACTIONS, Solver, as_bytes, read_u64},
};

type Policy = [f32; MAX_ACTIONS];

#[repr(C)]
#[derive(Clone, Copy, Deserialize, Serialize)]
struct InferenceNode {
    children: [usize; MAX_ACTIONS],
    policy_start: usize, // index into a Strategy list
}

pub struct InferenceSolver {
    mmap: memmap2::Mmap,
    card_history: usize,
    street_groups: [Box<dyn Group>; Street::COUNT],
}

impl InferenceSolver {
    fn get_node(&self, node: usize) -> &InferenceNode {
        unsafe {
            &*(self.mmap.as_ptr().add(node * size_of::<InferenceNode>()) as *const InferenceNode)
        }
    }

    fn get_strategy(&self, node: &InferenceNode, group: usize) -> Policy {
        unsafe {
            *(self
                .mmap
                .as_ptr()
                .add(node.policy_start + group * size_of::<Policy>())
                as *const Policy)
        }
    }

    const BUFFER_SIZE: usize = 64 * 1024;

    /// Writes mmap-able file to disk
    pub fn write_mmap(filename: &str, out: &str, groups: EngineGroups) {
        let mut writer =
            BufWriter::with_capacity(Self::BUFFER_SIZE, File::create_new(out).unwrap());
        let solver = Solver::load(filename, groups);

        let nodes = solver.nodes.len();
        let mut policy_start = nodes * size_of::<InferenceNode>();
        for node in solver.nodes.iter() {
            let inference_node = InferenceNode {
                children: node
                    .children
                    .iter()
                    .map(|c| c.unwrap_or(usize::MAX))
                    .collect::<Vec<usize>>()
                    .try_into()
                    .unwrap(),
                policy_start,
            };
            policy_start += node.policy.len() * size_of::<Policy>();

            writer.write_all(as_bytes(&inference_node)).unwrap();
        }

        for node in solver.nodes.iter() {
            for policy in node.policy.iter() {
                writer.write_all(as_bytes(&policy.strategy_sum)).unwrap();
            }
        }
    }

    pub fn load(filename: &str, groups: EngineGroups) -> Self {
        let mmap = unsafe { memmap2::Mmap::map(&File::open(filename).unwrap()).unwrap() };
        Self {
            mmap,
            street_groups: groups.groups,
            card_history: groups.card_history,
        }
    }

    fn group_streets(&self, street: Street) -> std::ops::Range<usize> {
        let end = street as usize + 1;
        end.saturating_sub(self.card_history)..end
    }

    pub fn lookup(&self, board: &Board, history: &[Action]) -> Actions {
        let mut node = 0;

        for action in history.iter() {
            // a bit hacky but whatever
            let index = match *action {
                Action::Fold => 0,
                Action::Call => 1,
                Action::Raise(_) => 2,
            };

            node = self.get_node(node).children[index];
            assert!(node != usize::MAX);
        }

        let cards = board.card_state();
        let mut group = 0;
        for s in self.group_streets(board.street()) {
            let street = Street::ALL[s];
            let community = cards.community[..street.visible_community()]
                .iter()
                .copied()
                .collect();

            let g = &self.street_groups[s];
            group = group * g.size() + g.map(cards.hands[board.stm() as usize], community)
        }

        let actions = board.get_actions().len();
        let strategies = self.get_strategy(self.get_node(node), group);
        let total: f64 = strategies[..actions].iter().map(|x| *x as f64).sum();
        let mut strategy = [0.0; MAX_ACTIONS];
        if total < f64::EPSILON {
            strategy[..actions].fill(1.0 / actions as f32);
        } else {
            for i in 0..actions {
                strategy[i] = (strategies[i] as f64 / total) as f32;
            }
        }
        strategy
    }
}
