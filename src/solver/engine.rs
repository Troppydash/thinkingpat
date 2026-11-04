use core::panic;
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    time::Instant,
};

use arrayvec::ArrayVec;
use fxhash::FxHashMap;
use rand::{
    Rng, RngExt,
    distr::{Distribution, weighted::WeightedIndex},
};
use serde::{Deserialize, Serialize};

use crate::{
    game::board::{Action, Board, CardState, Outcome, Player, Street},
    group::{
        equity::{
            FlopGroup, FlopGroupVersions, PreflopGroup, PreflopGroupVersions, RiverGroup,
            RiverGroupVersions, TurnGroup, TurnGroupVersions,
        },
        group::Group,
    },
    ver::{Versioned, try_load},
    versions,
};

pub const MAX_ACTIONS: usize = 3;
type Actions = [f32; MAX_ACTIONS];
type Belief = Vec<(Board, f64)>;
type BoardGroups = [[usize; Street::COUNT]; Player::COUNT];
type Response = FxHashMap<(usize, usize), usize>;

#[repr(C)]
#[derive(Default, Clone, Copy, Serialize, Deserialize)]
struct InfosetData {
    // TODO: consider fixed point
    regret_sum: Actions,
    strategy_sum: Actions,
}

fn adjust_sign(x: f64, stm: Player, child_stm: Player) -> f64 {
    if stm != child_stm { -x } else { x }
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

pub struct EngineGroups {
    pub groups: [Box<dyn Group>; Street::COUNT],
}

versions!(EngineGroupsVersions, V1, V2, V3);

impl Versioned<EngineGroupsVersions> for EngineGroups {
    fn build(version: &EngineGroupsVersions, path: &str) -> Self {
        match *version {
            EngineGroupsVersions::V1 => Self {
                groups: Street::ALL.map(|street| -> Box<dyn Group> {
                    match street {
                        Street::Preflop => {
                            Box::new(try_load::<PreflopGroupVersions, PreflopGroup>(
                                &PreflopGroupVersions::V1,
                                path,
                            ))
                        }
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
                }),
            },
            EngineGroupsVersions::V2 => Self {
                groups: Street::ALL.map(|street| -> Box<dyn Group> {
                    match street {
                        Street::Preflop => {
                            Box::new(try_load::<PreflopGroupVersions, PreflopGroup>(
                                &PreflopGroupVersions::V2,
                                path,
                            ))
                        }
                        Street::Flop => Box::new(try_load::<FlopGroupVersions, FlopGroup>(
                            &FlopGroupVersions::V2,
                            path,
                        )),
                        Street::Turn => Box::new(try_load::<TurnGroupVersions, TurnGroup>(
                            &TurnGroupVersions::V2,
                            path,
                        )),
                        Street::River => Box::new(try_load::<RiverGroupVersions, RiverGroup>(
                            &RiverGroupVersions::V2,
                            path,
                        )),
                    }
                }),
            },
            EngineGroupsVersions::V3 => Self {
                groups: Street::ALL.map(|street| -> Box<dyn Group> {
                    match street {
                        Street::Preflop => {
                            Box::new(try_load::<PreflopGroupVersions, PreflopGroup>(
                                &PreflopGroupVersions::V3,
                                path,
                            ))
                        }
                        Street::Flop => Box::new(try_load::<FlopGroupVersions, FlopGroup>(
                            &FlopGroupVersions::V3,
                            path,
                        )),
                        Street::Turn => Box::new(try_load::<TurnGroupVersions, TurnGroup>(
                            &TurnGroupVersions::V3,
                            path,
                        )),
                        Street::River => Box::new(try_load::<RiverGroupVersions, RiverGroup>(
                            &RiverGroupVersions::V3,
                            path,
                        )),
                    }
                }),
            },
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        unimplemented!()
    }

    fn from_bytes(_version: &EngineGroupsVersions, _path: &str, _bytes: &[u8]) -> Self {
        unimplemented!()
    }

    fn should_cache(_version: &EngineGroupsVersions) -> bool {
        false
    }
}

/// Note that the history is encapsulated in the children *pointers
#[derive(Clone, Serialize, Deserialize)]
struct BettingNode {
    // indices of action childs
    children: [Option<usize>; MAX_ACTIONS],
    // group indexed
    policy: Vec<InfosetData>,
}

pub struct Solver {
    // TODO: this can be reordered for perf
    nodes: Vec<BettingNode>,
    street_groups: [Box<dyn Group>; Street::COUNT],
    iters: usize,
}

impl Solver {
    pub fn new(groups: EngineGroups) -> Self {
        Self {
            nodes: vec![],
            street_groups: groups.groups,
            iters: 0,
        }
    }

    fn estimate_tree(&self) -> usize {
        2usize.pow(4 * 4)
    }

    fn estimate_memory(&self) -> usize {
        self.street_groups
            .iter()
            .map(|g| g.size())
            .product::<usize>()
            * self.estimate_tree()
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

    fn traverse(&mut self, board: &Board) -> Option<usize> {
        if board.outcome() != Outcome::Ongoing {
            return None;
        }

        let groups: usize = Street::ALL
            .iter()
            .filter(|&s| s <= &board.street())
            .map(|&s| self.street_groups[s as usize].size())
            .product();
        let node_id = self.nodes.len();
        self.nodes.push(BettingNode {
            children: [None; MAX_ACTIONS],
            policy: vec![InfosetData::default(); groups],
        });

        let actions = board.get_actions();
        for (i, action) in actions.iter().enumerate() {
            let mut new_board = board.clone();
            new_board.play(*action);
            self.nodes[node_id].children[i] = self.traverse(&new_board);
        }

        Some(node_id)
    }

    fn make_betting_tree(&mut self) {
        self.nodes.clear();
        self.nodes.reserve(self.estimate_tree());
        self.traverse(&Board::new());
    }

    #[inline]
    fn regret_matching(
        &mut self,
        data: &InfosetData,
        actions: usize,
        rng: &mut impl Rng,
    ) -> Actions {
        let regrets = &data.regret_sum;
        let total: f64 = regrets[..actions].iter().map(|x| x.max(0.0) as f64).sum();
        let mut strategy = [0.0; MAX_ACTIONS];
        if total < f64::EPSILON || rng.random_ratio(1, 10000) {
            strategy[..actions].fill(1.0 / actions as f32);
        } else {
            for i in 0..actions {
                strategy[i] = (regrets[i].max(0.0) as f64 / total) as f32;
            }
        }
        strategy
    }

    #[inline]
    fn average_strategy(&self, data: &InfosetData, actions: usize) -> Actions {
        let strategies = &data.strategy_sum;
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

    fn prefetch(&self, child_node: Option<usize>, new_board: &Board, board_group: &BoardGroups) {
        if let Some(child_node) = child_node {
            let child_group = board_group[new_board.stm() as usize][new_board.street() as usize];
            #[cfg(target_arch = "x86_64")]
            {
                unsafe {
                    use std::arch::x86_64::{_MM_HINT_ET0, _mm_prefetch};
                    _mm_prefetch(
                        self.nodes[child_node].policy.as_ptr().add(child_group) as *const i8,
                        _MM_HINT_ET0,
                    );
                }
            }

            #[cfg(target_arch = "aarch64")]
            {
                unsafe {
                    use core::arch::aarch64::{_PREFETCH_LOCALITY0, _PREFETCH_WRITE, _prefetch};
                    _prefetch::<_PREFETCH_WRITE, _PREFETCH_LOCALITY0>(
                        self.nodes[child_node].policy.as_ptr().add(child_group) as *const i8,
                    );
                }
            }
        }
    }

    fn cfr(
        &mut self,
        reach_prob: &[f64; Player::COUNT],
        board: &Board,
        node: Option<usize>,
        board_group: &BoardGroups,
        rng: &mut impl Rng,
    ) -> f64 {
        // terminal
        if let Outcome::Winnings(winnings) = board.outcome() {
            return winnings[board.stm() as usize] as f64;
        }

        let node = node.unwrap();

        // prune
        if reach_prob.iter().all(|&p| p < f64::EPSILON) {
            return 0.0;
        }

        let actions = board.get_actions();
        let group = board_group[board.stm() as usize][board.street() as usize];
        let data = self.nodes[node].policy[group];

        let mut new_boards: ArrayVec<Board, MAX_ACTIONS> = ArrayVec::new();
        for (i, action) in actions.iter().enumerate() {
            let mut new_board = board.clone();
            new_board.play(*action);

            let child_node = self.nodes[node].children[i];
            self.prefetch(child_node, &new_board, board_group);
            new_boards.push(new_board);
        }

        let strategy = self.regret_matching(&data, actions.len(), rng);

        let mut counterfactual_value = [0.0_f64; MAX_ACTIONS];
        for (i, new_board) in new_boards.iter().enumerate() {
            let child_node = self.nodes[node].children[i];

            let mut reach_prob = reach_prob.clone();
            reach_prob[board.stm() as usize] *= strategy[i] as f64;

            counterfactual_value[i] = adjust_sign(
                self.cfr(&reach_prob, new_board, child_node, board_group, rng),
                board.stm(),
                new_board.stm(),
            );
        }

        let value: f64 = counterfactual_value
            .iter()
            .enumerate()
            .map(|(i, v)| strategy[i] as f64 * v)
            .sum();

        // TODO: optimize this via cut/prune

        // regret sum update
        let data = &mut self.nodes[node].policy[group];
        for i in 0..actions.len() {
            let opp_reach_prob = reach_prob[!board.stm() as usize];

            // CFR+
            data.regret_sum[i] = (data.regret_sum[i]
                + (opp_reach_prob * (counterfactual_value[i] - value)) as f32)
                .max(0.0);

            let t = self.iters as f64;
            data.strategy_sum[i] +=
                (t * reach_prob[board.stm() as usize] * strategy[i] as f64) as f32;
        }

        return value;
    }

    fn can_regret_prune(&self) -> bool {
        self.iters % 20 != 0
    }

    fn mc_cfr(
        &mut self,
        responder: Player,
        board: &Board,
        node: Option<usize>,
        board_group: &BoardGroups,
        rng: &mut impl Rng,
    ) -> f64 {
        // terminal
        // TODO: we actually don't need board
        if let Outcome::Winnings(winnings) = board.outcome() {
            return winnings[responder as usize] as f64;
        }

        let node = node.unwrap();

        let actions = board.get_actions();
        let group = board_group[board.stm() as usize][board.street() as usize];
        let data = self.nodes[node].policy[group];

        let strategy = self.regret_matching(&data, actions.len(), rng);

        let value: f64;
        if board.stm() != responder {
            let dist = WeightedIndex::new(&strategy).unwrap();
            let sampled = dist.sample(rng);
            let action = actions[sampled];

            let mut new_board = board.clone();
            new_board.play(action);

            let child_node = self.nodes[node].children[sampled];
            value = self.mc_cfr(responder, &new_board, child_node, board_group, rng);

            let t = self.iters as f64;
            let data = &mut self.nodes[node].policy[group];
            for i in 0..actions.len() {
                data.strategy_sum[i] += (t * strategy[i] as f64) as f32;
            }
        } else {
            let mut counterfactual_value = [0.0_f64; MAX_ACTIONS];
            let regret_sum = self.nodes[node].policy[group].regret_sum;
            let mut pruned = [false; MAX_ACTIONS];
            for (i, action) in actions.iter().enumerate() {
                // we have eps exploration so the strategy[i] check is needed
                if self.can_regret_prune()
                    && strategy[i] <= f32::EPSILON
                    && regret_sum[i] < -10000.0
                {
                    // regret prune
                    pruned[i] = true;
                    continue;
                }

                let mut new_board = board.clone();
                new_board.play(*action);

                let child_node = self.nodes[node].children[i];
                self.prefetch(child_node, &new_board, board_group);

                counterfactual_value[i] =
                    self.mc_cfr(responder, &new_board, child_node, board_group, rng);
            }

            value = counterfactual_value
                .iter()
                .enumerate()
                .map(|(i, v)| strategy[i] as f64 * v)
                .sum();

            // regret sum update
            let data = &mut self.nodes[node].policy[group];
            for i in 0..actions.len() {
                if pruned[i] {
                    continue;
                }

                data.regret_sum[i] = data.regret_sum[i] + (counterfactual_value[i] - value) as f32;
            }
        }

        return value;
    }

    fn build_response(
        &self,
        responder: Player,
        belief: Belief,
        node: Option<usize>,
        board_groups: &FxHashMap<CardState, BoardGroups>,
        response: &mut Response,
    ) -> f64 {
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

        let mut infosets: FxHashMap<usize, Belief> = FxHashMap::default();
        for (board, weight) in belief {
            let group = board_groups[&board.card_state_full()][board.stm() as usize]
                [board.street() as usize];

            infosets.entry(group).or_default().push((board, weight));
        }

        let node = node.unwrap();

        if stm == responder {
            infosets
                .into_iter()
                .map(|(group, states)| {
                    let mut total = f64::NEG_INFINITY;
                    let mut best = 0;
                    for (i, action) in actions.iter().enumerate() {
                        let children = states
                            .iter()
                            .map(|(board, weight)| {
                                let mut child = board.clone();
                                child.play(*action);
                                (child, *weight)
                            })
                            .collect();

                        let child_node = self.nodes[node].children[i];
                        let value = self.build_response(
                            responder,
                            children,
                            child_node,
                            board_groups,
                            response,
                        );

                        if value > total {
                            total = value;
                            best = i;
                        }
                    }

                    response.insert((node, group), best);
                    total
                })
                .sum()
        } else {
            let mut children: [Belief; MAX_ACTIONS] = [const { vec![] }; MAX_ACTIONS];
            for (group, states) in infosets {
                let strategy =
                    self.average_strategy(&self.nodes[node].policy[group], actions.len());
                for (board, weight) in states.iter() {
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
            for (i, states) in children.into_iter().enumerate() {
                if states.is_empty() {
                    continue;
                }
                let child_node = self.nodes[node].children[i];
                total += self.build_response(responder, states, child_node, board_groups, response);
            }
            total
        }
    }

    fn response_value(
        &self,
        responder: Player,
        mut board: Board,
        board_groups: &BoardGroups,
        response: &Response,
        rng: &mut impl Rng,
    ) -> f64 {
        let mut node = Some(0);
        loop {
            if let Outcome::Winnings(winnings) = board.outcome() {
                return winnings[responder as usize] as f64;
            }

            let node_id = node.unwrap();
            let actions = board.get_actions();
            let group = board_groups[board.stm() as usize][board.street() as usize];
            let best = if board.stm() == responder {
                response.get(&(node_id, group))
            } else {
                None
            };
            let sampled = if let Some(&best) = best {
                best
            } else {
                let strategy =
                    self.average_strategy(&self.nodes[node_id].policy[group], actions.len());
                let dist = WeightedIndex::new(&strategy[..actions.len()]).unwrap();
                dist.sample(rng)
            };

            board.play(actions[sampled]);
            node = self.nodes[node_id].children[sampled];
        }
    }

    fn response_strength(&self, iters: usize) -> (f64, f64) {
        let boards: Vec<_> = (0..iters).map(|_| Board::new()).collect();
        let board_groups = self.make_board_groups(&boards);
        let responses = Player::ALL.map(|responder| {
            let mut response = Response::default();
            let belief = boards
                .iter()
                .map(|board| (board.clone(), 1.0 / iters as f64))
                .collect();
            self.build_response(responder, belief, Some(0), &board_groups, &mut response);
            response
        });

        let mut rng = rand::rng();
        let mut mean = 0.0;
        let mut m2 = 0.0;
        for iter in 0..iters {
            let board = Board::new();
            let board_groups = self.make_board_group(&board);
            let value = Player::ALL
                .into_iter()
                .map(|responder| {
                    self.response_value(
                        responder,
                        board.clone(),
                        &board_groups,
                        &responses[responder as usize],
                        &mut rng,
                    )
                })
                .sum::<f64>()
                / 2.0;

            let n = (iter + 1) as f64;
            let delta = value - mean;
            mean += delta / n;
            m2 += delta * (value - mean);
        }

        let variance = m2 / (iters - 1).max(1) as f64;
        (mean, 1.96 * (variance / iters as f64).sqrt())
    }

    pub fn solve(&mut self, filename: &str, iters: usize) {
        if iters == 0 {
            return;
        }

        const ITER_PRINT: usize = 20_000;
        // aim for per 2-3 min
        const ITER_SAVE: usize = 5_000_000;
        // aim for per min
        const ITER_RESPONSE: usize = 5_000_000;

        std::fs::create_dir_all(filename).unwrap();
        println!(
            "info cfr estimate_memory {}mb",
            self.estimate_memory() / 1024 / 1024
        );

        if self.nodes.is_empty() {
            println!("info cfr making betting tree");
            self.make_betting_tree();
        }

        let mut rng = rand::rng();

        let target = self.iters + iters;
        let start = Instant::now();
        while self.iters < target {
            self.iters += 1;

            if self.iters % ITER_PRINT == 0 {
                println!(
                    "info cfr iter {} iter/s {}",
                    self.iters,
                    self.iters as u64 * 1000 / (start.elapsed().as_millis() as u64).max(1)
                );
            }

            // let reach_prob = [1.0; Player::COUNT];
            // self.cfr(&reach_prob, &board, Some(0), &board_groups, &mut rng);

            for player in Player::ALL {
                let board = Board::new();
                let board_groups = self.make_board_group(&board);
                self.mc_cfr(player, &board, Some(0), &board_groups, &mut rng);
            }

            if self.iters % ITER_SAVE == 0 || self.iters == target {
                self.save(&format!("{filename}/iter_{}.bin", self.iters));
            }
            if self.iters % ITER_RESPONSE == 0 {
                let (mean, error) = self.response_strength(100000);
                println!("info cfr response_strength {mean:.5} +- {error:.5} sb");
            }
        }
    }

    const CHECKPOINT_BUFFER_SIZE: usize = 64 * 1024;

    fn save(&self, filename: &str) {
        let writer = BufWriter::with_capacity(
            Self::CHECKPOINT_BUFFER_SIZE,
            File::create(filename).unwrap(),
        );
        Self::write_checkpoint(&self.nodes, writer).unwrap();
    }

    fn write_checkpoint(nodes: &[BettingNode], writer: impl Write) -> io::Result<()> {
        let mut encoder = zstd::stream::write::Encoder::new(writer, 7)?;
        encoder.write_all(&(nodes.len() as u64).to_le_bytes())?;

        for data in nodes {
            let raw = postcard::to_allocvec(data)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            encoder.write_all(&(raw.len() as u64).to_le_bytes())?;
            encoder.write_all(&raw)?;
        }

        encoder.finish()?.flush()
    }

    pub fn load(filename: &str, groups: EngineGroups) -> Self {
        let reader =
            BufReader::with_capacity(Self::CHECKPOINT_BUFFER_SIZE, File::open(filename).unwrap());
        let mut solver = Self::new(groups);
        solver.nodes = Self::read_checkpoint(reader);
        solver
    }

    fn read_checkpoint(reader: impl Read) -> Vec<BettingNode> {
        let mut reader = zstd::stream::read::Decoder::new(reader).unwrap();
        let count = read_u64(&mut reader).unwrap() as usize;
        let mut nodes = Vec::with_capacity(count);
        let mut bytes = vec![];

        for _ in 0..count {
            let size = read_u64(&mut reader).unwrap() as usize;
            bytes.resize(size, 0);
            reader.read_exact(&mut bytes).unwrap();
            nodes.push(postcard::from_bytes(&bytes).unwrap());
        }

        nodes
    }

    pub fn lookup(&mut self, board: &Board, history: &[Action]) -> Actions {
        let mut node = 0;

        for action in history.iter() {
            // a bit hacky but whatever
            let index = match *action {
                Action::Fold => 0,
                Action::Call => 1,
                Action::Raise(_) => 2,
            };

            node = self.nodes[node].children[index].unwrap();
        }

        let cards = board.card_state();
        let mut group = 0;
        for street in Street::ALL {
            if street > board.street() {
                break;
            }

            let community = cards.community[..street.visible_community()]
                .iter()
                .copied()
                .collect();

            let g = &self.street_groups[street as usize];
            group = group * g.size() + g.map(cards.hands[board.stm() as usize], community)
        }

        self.average_strategy(&self.nodes[node].policy[group], board.get_actions().len())
    }
}
