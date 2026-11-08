use std::{fs::File, io::BufReader};

use crate::{
    game::board::{Action, Board, Street},
    group::group::Group,
    solver::engine::{Actions, EngineGroups, MAX_ACTIONS, Solver, read_u64},
};

type Policy = [f32; MAX_ACTIONS];

#[repr(C)]
#[derive(Clone, Copy)]
struct InferenceNode {
    children: [usize; MAX_ACTIONS],
    policy_start: usize, // index into a Strategy list
}

pub struct InferenceSolver {
    mmap: memmap2::Mmap,
    nodes: usize,
    policy_offset: usize,
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

    pub fn load(filename: &str, groups: EngineGroups) -> Self {
        let make_reader =
            || BufReader::with_capacity(Self::BUFFER_SIZE, File::open(filename).unwrap());

        // count nodes + policies
        let (node_count, policy_count) = {
            let reader = make_reader();
            let mut policy_count = 0;
            let metadata = Solver::stream_checkpoint(reader, |node| {
                policy_count += node.policy.len();
            });

            (metadata.nodes, policy_count)
        };

        let policy_offset = node_count * size_of::<InferenceNode>();
        let data_size = policy_offset + policy_count * size_of::<Policy>();

        let mut mmap = memmap2::MmapMut::map_anon(data_size).unwrap();
        let mut node_index = 0;
        let mut policy_index = policy_offset;

        let reader = make_reader();
        let _ = Solver::stream_checkpoint(reader, |node| {
            let inference_node = InferenceNode {
                children: node
                    .children
                    .iter()
                    .map(|&n| n.unwrap_or(usize::MAX))
                    .collect::<Vec<usize>>()
                    .try_into()
                    .unwrap(),
                policy_start: policy_index,
            };

            unsafe {
                let ptr = mmap.as_mut_ptr().add(node_index) as *mut InferenceNode;
                ptr.write(inference_node);

                // then policy
                for data in node.policy {
                    let policy = data.strategy_sum;
                    let ptr = mmap.as_mut_ptr().add(policy_index) as *mut Policy;
                    ptr.write(policy);
                    policy_index += size_of::<Policy>();
                }
            }

            node_index += size_of::<InferenceNode>();
        });

        let mmap = mmap.make_read_only().unwrap();

        Self {
            mmap,
            nodes: node_count,
            policy_offset,
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
