use core::panic;
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
};

use arrayvec::ArrayVec;
use fxhash::FxHashMap;
use rand::{
    Rng, RngExt,
    distr::{Distribution, weighted::WeightedIndex},
};
use smallvec::SmallVec;

use crate::{
    game::{
        board::{Action, Board, CardState, Outcome, Player, Street},
        card::CardSet,
    },
    group::{
        equity::{
            FlopGroup, FlopGroupVersions, PreflopGroup, PreflopGroupVersions, RiverGroup,
            RiverGroupVersions, TurnGroup, TurnGroupVersions,
        },
        group::Group,
    },
    ver::try_load,
};

pub const MAX_ACTION_LENGTH: usize = 4 * 5;
pub const MAX_ACTIONS: usize = 3;
type Actions = [f32; MAX_ACTIONS];
type Policy = FxHashMap<Infoset, InfosetData>;

// arbitrary 20
type Belief = SmallVec<[(Board, f64); 20]>;

const CHECKPOINT_BUFFER_SIZE: usize = 64 * 1024;

type BoardGroups = [[usize; Street::COUNT]; Player::COUNT];

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct InfosetData {
    // TODO: consider fixed point
    regret_sum: Actions,
    strategy_sum: Actions,
}

#[repr(C)]
#[derive(Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct Infoset {
    // this is the mixed radix group index
    group: u32,
    history: u32,
    street: u8,
    _padding: [u8; 3],
}

impl Infoset {
    fn new(street: Street, group: usize, history: &[Action]) -> Self {
        let mut compressed_history = 1u32 << history.len();

        for (i, action) in history.iter().enumerate() {
            match *action {
                Action::Fold => panic!("unexpected fold"),
                Action::Call => compressed_history |= 1 << i,
                Action::Raise(_) => {}
            }
        }

        Self {
            street: street as u8,
            group: u32::try_from(group).expect("group index exceeds u32"),
            history: compressed_history,
            _padding: [0; 3],
        }
    }
}

#[repr(C)]
#[derive(Default)]
struct CheckpointEntry {
    infoset: Infoset,
    data: InfosetData,
}

const _: () = assert!(size_of::<Infoset>() == 2 * size_of::<u32>() + 4);
const _: () = assert!(size_of::<InfosetData>() == 2 * size_of::<Actions>());
const _: () =
    assert!(size_of::<CheckpointEntry>() == size_of::<Infoset>() + size_of::<InfosetData>());

fn adjust_sign(x: f64, stm: Player, child_stm: Player) -> f64 {
    if stm != child_stm { -x } else { x }
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

// TODO: make this a parameter instead
pub fn create_groups(path: &str) -> [Box<dyn Group>; Street::COUNT] {
    Street::ALL.map(|street| -> Box<dyn Group> {
        match street {
            Street::Preflop => Box::new(try_load::<PreflopGroupVersions, PreflopGroup>(
                &PreflopGroupVersions::V1,
                path,
            )),
            Street::Flop => Box::new(try_load::<FlopGroupVersions, FlopGroup>(
                &FlopGroupVersions::V1,
                path,
            )),
            Street::Turn => Box::new(try_load::<TurnGroupVersions, TurnGroup>(
                &TurnGroupVersions::V1,
                path,
            )),
            Street::River => Box::new(try_load::<RiverGroupVersions, RiverGroup>(
                &RiverGroupVersions::V1,
                path,
            )),
        }
    })
}

pub struct Solver {
    infosets: Policy,
    street_groups: [Box<dyn Group>; Street::COUNT],
    iters: usize,
}

impl Solver {
    pub fn new(path: &str) -> Self {
        Self {
            infosets: FxHashMap::default(),
            street_groups: create_groups(path),
            iters: 0,
        }
    }

    fn make_board_group(&self, board: &Board) -> BoardGroups {
        let mut groups = [[0; Street::COUNT]; Player::COUNT];
        let cards = board.card_state_full();
        for player in Player::ALL {
            for street in Street::ALL {
                let hand = cards.hands[player as usize];
                let community = cards.community[..street.visible_community()]
                    .iter()
                    .copied()
                    .collect();

                let group = self.street_groups[street as usize].map(hand, community);

                let total_group = if street == Street::Preflop {
                    group
                } else {
                    group
                        + groups[player as usize][street as usize - 1]
                            * self.street_groups[street as usize].size()
                };

                groups[player as usize][street as usize] = total_group;
            }
        }

        groups
    }

    fn make_board_groups(&self, boards: &[Board]) -> FxHashMap<CardState, BoardGroups> {
        let mut output: FxHashMap<CardState, BoardGroups> = FxHashMap::default();

        for board in boards.iter() {
            if output.contains_key(&board.card_state_full()) {
                continue;
            }

            let groups = self.make_board_group(board);

            output.insert(board.card_state_full(), groups);
        }

        output
    }

    #[inline]
    fn regret_matching(&mut self, infoset: &Infoset, actions: usize) -> Actions {
        let regrets = &self.infosets.entry(infoset.clone()).or_default().regret_sum;
        let total: f64 = regrets[..actions].iter().map(|x| x.max(0.0) as f64).sum();
        let mut strategy = [0.0; MAX_ACTIONS];
        if total < f64::EPSILON || rand::rng().random_ratio(1, 10000) {
            strategy[..actions].fill(1.0 / actions as f32);
        } else {
            for i in 0..actions {
                strategy[i] = (regrets[i].max(0.0) as f64 / total) as f32;
            }
        }
        strategy
    }

    #[inline]
    pub fn average_strategy(&self, infoset: &Infoset, actions: usize) -> Actions {
        let strategies = self
            .infosets
            .get(infoset)
            .map_or([0.0; MAX_ACTIONS], |data| data.strategy_sum);
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

    fn cfr(
        &mut self,
        reach_prob: &[f64; Player::COUNT],
        board: &Board,
        history: &mut ArrayVec<Action, MAX_ACTION_LENGTH>,
        board_group: &BoardGroups,
    ) -> f64 {
        // terminal
        if let Outcome::Winnings(winnings) = board.outcome() {
            return winnings[board.stm() as usize] as f64;
        }

        // prune
        if reach_prob.iter().all(|&p| p < f64::EPSILON) {
            return 0.0;
        }

        let infoset = Infoset::new(
            board.street(),
            board_group[board.stm() as usize][board.street() as usize],
            &history,
        );

        let actions = board.get_actions();
        let strategy = self.regret_matching(&infoset, actions.len());

        let mut counterfactual_value = [0.0_f64; MAX_ACTIONS];
        for (i, action) in actions.iter().enumerate() {
            let mut new_board = board.clone();
            new_board.play(*action);

            history.push(*action);

            let mut reach_prob = reach_prob.clone();
            reach_prob[board.stm() as usize] *= strategy[i] as f64;

            counterfactual_value[i] = adjust_sign(
                self.cfr(&reach_prob, &new_board, history, board_group),
                board.stm(),
                new_board.stm(),
            );
            history.pop();
        }

        let value: f64 = counterfactual_value
            .iter()
            .enumerate()
            .map(|(i, v)| strategy[i] as f64 * v)
            .sum();

        // TODO: optimize this via cut/prune

        // regret sum update
        let data = self.infosets.get_mut(&infoset).unwrap();
        for i in 0..actions.len() {
            let opp_reach_prob = reach_prob[!board.stm() as usize];
            data.regret_sum[i] = (data.regret_sum[i]
                + (opp_reach_prob * (counterfactual_value[i] - value)) as f32)
                .max(0.0);
            data.strategy_sum[i] += (reach_prob[board.stm() as usize] * strategy[i] as f64) as f32;
        }

        return value;
    }

    fn mc_cfr(
        &mut self,
        responder: Player,
        board: &Board,
        history: &mut ArrayVec<Action, MAX_ACTION_LENGTH>,
        board_group: &BoardGroups,
        rng: &mut impl Rng,
    ) -> f64 {
        // terminal
        if let Outcome::Winnings(winnings) = board.outcome() {
            return winnings[responder as usize] as f64;
        }

        let infoset = Infoset::new(
            board.street(),
            board_group[board.stm() as usize][board.street() as usize],
            &history,
        );

        let actions = board.get_actions();
        let strategy = self.regret_matching(&infoset, actions.len());

        let value: f64;

        if board.stm() != responder {
            let dist = WeightedIndex::new(&strategy).unwrap();
            let sampled = dist.sample(rng);
            let action = actions[sampled];

            let mut new_board = board.clone();
            new_board.play(action);

            history.push(action);
            value = self.mc_cfr(responder, &new_board, history, board_group, rng);
            history.pop();

            let data = self.infosets.get_mut(&infoset).unwrap();
            for i in 0..actions.len() {
                data.strategy_sum[i] += strategy[i] as f32;
            }
        } else {
            let mut counterfactual_value = [0.0_f64; MAX_ACTIONS];
            for (i, action) in actions.iter().enumerate() {
                let mut new_board = board.clone();
                new_board.play(*action);

                history.push(*action);

                counterfactual_value[i] =
                    self.mc_cfr(responder, &new_board, history, board_group, rng);
                history.pop();
            }

            value = counterfactual_value
                .iter()
                .enumerate()
                .map(|(i, v)| strategy[i] as f64 * v)
                .sum();

            // regret sum update
            let data = self.infosets.get_mut(&infoset).unwrap();
            for i in 0..actions.len() {
                data.regret_sum[i] =
                    (data.regret_sum[i] + (counterfactual_value[i] - value) as f32);
            }
        }

        return value;
    }

    fn best_response_evaluate(
        &mut self,
        responder: Player,
        // weights are all !responder multiplied
        belief: Belief,
        history: &mut SmallVec<[Action; MAX_ACTION_LENGTH]>,
        board_groups: &FxHashMap<CardState, BoardGroups>,
    ) -> f64 {
        // all the boards are synced
        let board = &belief[0].0;
        if let Outcome::Winnings(_) = board.outcome() {
            return belief
                .iter()
                .map(|(board, weight)| {
                    let Outcome::Winnings(winnings) = board.outcome() else {
                        panic!("best-response deals must share a public betting history");
                    };
                    weight * winnings[responder as usize] as f64
                })
                .sum();
        }

        let actions = board.get_actions();
        let stm = board.stm();
        assert!(
            belief.iter().all(|(other, _)| {
                other.outcome() == Outcome::Ongoing
                    && other.street() == board.street()
                    && other.stm() == stm
                    && other.get_actions() == actions
            }),
            "best-response deals must share a public betting history"
        );

        // group boards by infosets
        let mut infosets: FxHashMap<Infoset, Belief> = FxHashMap::default();
        for (board, weight) in belief {
            let infoset = Infoset::new(
                board.street(),
                board_groups[&board.card_state_full()][board.stm() as usize]
                    [board.street() as usize],
                &history,
            );

            infosets.entry(infoset).or_default().push((board, weight));
        }

        if stm == responder {
            infosets
                .into_values()
                .map(|states| {
                    assert!(!states.is_empty());

                    let mut total = f64::NEG_INFINITY;
                    for action in actions.iter() {
                        let children = states
                            .iter()
                            .map(|(board, weight)| {
                                let mut child = board.clone();
                                child.play(*action);
                                (child, *weight)
                            })
                            .collect();
                        history.push(*action);
                        let value =
                            self.best_response_evaluate(responder, children, history, board_groups);
                        history.pop();

                        total = total.max(value);
                    }

                    total
                })
                .sum()
        } else {
            // non responder doesn't partition by infoset because that is private information
            let mut children: SmallVec<[Belief; MAX_ACTIONS]> =
                (0..actions.len()).map(|_| SmallVec::new()).collect();
            for (infoset, states) in infosets {
                let strategy = self.average_strategy(&infoset, actions.len());
                for (board, weight) in states {
                    for (i, action) in actions.iter().enumerate() {
                        let child_weight = weight * strategy[i] as f64;
                        if child_weight > 0.0 {
                            let mut child = board.clone();
                            child.play(*action);
                            children[i].push((child, child_weight));
                        }
                    }
                }
            }

            let mut total = 0.0;
            for (action, states) in actions.iter().zip(children) {
                if states.is_empty() {
                    continue;
                }
                history.push(*action);
                total += self.best_response_evaluate(responder, states, history, board_groups);
                history.pop();
            }
            total
        }
    }

    fn best_response(
        &mut self,
        responder: Player,
        boards: &[Board],
        history: &mut SmallVec<[Action; MAX_ACTION_LENGTH]>,
        board_groups: &FxHashMap<CardState, BoardGroups>,
    ) -> f64 {
        let chance_weight = 1.0 / boards.len() as f64;
        let belief = boards
            .iter()
            .map(|board| (board.clone(), chance_weight))
            .collect();
        self.best_response_evaluate(responder, belief, history, board_groups)
    }

    fn exploitability(&mut self, iters: usize) -> f64 {
        let boards: Vec<_> = (0..iters).map(|_| Board::new()).collect();
        let board_groups = self.make_board_groups(&boards);

        let mut history: SmallVec<[Action; MAX_ACTION_LENGTH]> = SmallVec::new();
        Player::ALL
            .into_iter()
            .map(|p| self.best_response(p, &boards, &mut history, &board_groups))
            .sum::<f64>()
            / 2.0
    }

    pub fn solve(&mut self, filename: &str, iters: usize) {
        if iters == 0 {
            return;
        }

        std::fs::create_dir_all(filename).unwrap();
        let mut history: ArrayVec<Action, MAX_ACTION_LENGTH> = ArrayVec::new();
        let states: usize = self.street_groups.iter().map(|s| s.size()).product();
        println!("info cfr states at least {states}");

        let mut rng = rand::rng();

        let target = self.iters + iters;
        while self.iters < target {
            if self.iters % 5000 == 0 {
                println!("info cfr iter {}", self.iters);
            }
            let board = Board::new();
            let board_groups = self.make_board_group(&board);
            let reach_prob = [1.0; Player::COUNT];
            self.cfr(&reach_prob, &board, &mut history, &board_groups);
            // for player in Player::ALL {
            //     self.mc_cfr(player, &board, &mut history, &board_groups, &mut rng);
            // }

            self.iters += 1;

            if self.iters % 200000 == 0 || self.iters == target {
                self.save(&format!("{filename}/iter_{}.bin", self.iters));
            }
            if self.iters % 200000 == 0 {
                println!(
                    "info cfr exploitability {:.5} sb",
                    self.exploitability(20000)
                );
            }
        }
    }

    pub fn save(&self, filename: &str) {
        let mut writer =
            BufWriter::with_capacity(CHECKPOINT_BUFFER_SIZE, File::create(filename).unwrap());
        writer
            .write_all(&(self.infosets.len() as u64).to_le_bytes())
            .unwrap();

        for (infoset, data) in &self.infosets {
            let record = CheckpointEntry {
                infoset: *infoset,
                data: *data,
            };
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    (&record as *const CheckpointEntry).cast::<u8>(),
                    size_of::<CheckpointEntry>(),
                )
            };
            writer.write_all(bytes).unwrap();
        }

        writer.flush().unwrap()
    }

    pub fn load(filename: &str, path: &str) -> Self {
        let mut reader =
            BufReader::with_capacity(CHECKPOINT_BUFFER_SIZE, File::open(filename).unwrap());
        let mut solver = Self::new(path);
        let count = read_u64(&mut reader).unwrap() as usize;
        solver.infosets.try_reserve(count).unwrap();

        for _ in 0..count {
            let mut record = CheckpointEntry::default();

            let bytes = unsafe {
                std::slice::from_raw_parts_mut(
                    (&mut record as *mut CheckpointEntry).cast::<u8>(),
                    size_of::<CheckpointEntry>(),
                )
            };
            reader.read_exact(bytes).unwrap();
            solver.infosets.insert(record.infoset, record.data);
        }

        solver
    }

    pub fn lookup(&mut self, board: &Board, history: &[Action]) -> Actions {
        let card = board.card_state();
        let hand = card.hands[board.stm() as usize];
        let mut group = self.street_groups[Street::Preflop as usize].map(hand, CardSet::EMPTY);
        for street in 1..=(board.street() as usize) {
            group = group * self.street_groups[street].size()
                + self.street_groups[street].map(
                    hand,
                    card.community[..Street::ALL[street].visible_community()]
                        .iter()
                        .cloned()
                        .collect(),
                );
        }

        let infoset = Infoset::new(board.street(), group, history);
        self.average_strategy(&infoset, board.get_actions().len())
    }
}
