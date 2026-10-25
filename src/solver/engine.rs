use fxhash::FxHashMap;
use smallvec::{SmallVec, smallvec};

use crate::{
    game::{
        board::{Action, Board, CardState, Outcome, Player, Street},
        card::CardSet,
    },
    group::group::Group,
};

pub const MAX_ACTION_LENGTH: usize = 8;
pub const MAX_ACTIONS: usize = 3;
type Actions = SmallVec<[f64; MAX_ACTIONS]>;
type Policy = FxHashMap<Infoset, Actions>;
// arbitrary 20
type Belief = SmallVec<[(Board, f64); 20]>;

type BoardGroups = [[usize; Street::COUNT]; Player::COUNT];

#[derive(Hash, PartialEq, Eq, Clone)]
struct Infoset {
    street: Street,
    group: usize,
    history: SmallVec<[Action; MAX_ACTION_LENGTH]>,
}

fn get_or_default<'a>(table: &'a mut Policy, key: &Infoset, actions: usize) -> &'a mut Actions {
    if !table.contains_key(key) {
        let value = smallvec![0.0; actions];
        table.insert(key.clone(), value);
    }

    table.get_mut(&key).unwrap()
}

fn adjust_sign(x: f64, stm: Player, child_stm: Player) -> f64 {
    if stm != child_stm { -x } else { x }
}

fn make_board_groups(
    boards: &[Board],
    map: &[Box<dyn Group>; Street::COUNT],
) -> FxHashMap<CardState, BoardGroups> {
    let mut output: FxHashMap<CardState, BoardGroups> = FxHashMap::default();

    for board in boards.iter() {
        if output.contains_key(&board.card_state_full()) {
            continue;
        }

        let mut groups = [[0; Street::COUNT]; Player::COUNT];
        let cards = board.card_state_full();
        for player in Player::ALL {
            for street in Street::ALL {
                let group = map[street as usize].map(
                    cards.hands[player as usize],
                    CardSet::from_iter(
                        cards.community[..street.visible_community()]
                            .iter()
                            .cloned(),
                    ),
                );
                groups[player as usize][street as usize] = group;
            }
        }

        output.insert(board.card_state_full(), groups);
    }

    output
}

pub struct Solver {
    regret_sum: Policy,
    strategy_sum: Policy,
    street_groups: [Box<dyn Group>; Street::COUNT],
    iters: usize,
}

impl Solver {
    pub fn new(load: impl Fn(Street) -> Box<dyn Group>) -> Self {
        Self {
            regret_sum: FxHashMap::default(),
            strategy_sum: FxHashMap::default(),
            street_groups: Street::ALL.map(load),
            iters: 0,
        }
    }

    fn regret_matching(&mut self, infoset: &Infoset, actions: usize) -> Actions {
        let regrets = get_or_default(&mut self.regret_sum, infoset, actions);
        let total: f64 = regrets.iter().map(|x| x.max(0.0)).sum();
        if total < f64::EPSILON {
            smallvec![1.0 / actions as f64; actions]
        } else {
            regrets.iter().map(|x| x.max(0.0) / total).collect()
        }
    }

    fn average_strategy(&mut self, infoset: &Infoset, actions: usize) -> Actions {
        // TODO: change this to not [get_or_default]
        let strategies = get_or_default(&mut self.strategy_sum, infoset, actions);
        let total: f64 = strategies.iter().sum();
        if total < f64::EPSILON {
            smallvec![1.0 / actions as f64; actions]
        } else {
            strategies.iter().map(|x| x / total).collect()
        }
    }

    fn cfr(
        &mut self,
        reach_prob: &[f64; Player::COUNT],
        board: &Board,
        history: &mut SmallVec<[Action; MAX_ACTION_LENGTH]>,
        board_group: &BoardGroups,
    ) -> f64 {
        // terminal
        if let Outcome::Winnings(winnings) = board.outcome() {
            return winnings[board.stm() as usize] as f64;
        }

        let infoset = Infoset {
            street: board.street(),
            group: board_group[board.stm() as usize][board.street() as usize],
            history: history.clone(),
        };

        let actions = board.get_actions();
        let strategy = self.regret_matching(&infoset, actions.len());

        let mut counterfactual_value: Actions = smallvec![0.0; actions.len()];
        for (i, action) in actions.iter().enumerate() {
            let mut new_board = board.clone();
            new_board.play(*action);

            history.push(*action);

            let mut reach_prob = reach_prob.clone();
            reach_prob[board.stm() as usize] *= strategy[i];

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
            .map(|(i, v)| strategy[i] * v)
            .sum();

        // regret sum update
        let regrets = get_or_default(&mut self.regret_sum, &infoset, actions.len());
        let strategies = get_or_default(&mut self.strategy_sum, &infoset, actions.len());
        for i in 0..actions.len() {
            let opp_reach_prob = reach_prob[!board.stm() as usize];
            regrets[i] += opp_reach_prob * (counterfactual_value[i] - value);
            strategies[i] += reach_prob[board.stm() as usize] * strategy[i];
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
            let infoset = Infoset {
                street: board.street(),
                group: board_groups[&board.card_state_full()][board.stm() as usize]
                    [board.street() as usize],
                history: history.clone(),
            };

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
                        let child_weight = weight * strategy[i];
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
        let board_groups = make_board_groups(&boards, &self.street_groups);

        let mut history: SmallVec<[Action; MAX_ACTION_LENGTH]> = SmallVec::new();
        Player::ALL
            .into_iter()
            .map(|p| self.best_response(p, &boards, &mut history, &board_groups))
            .sum::<f64>()
            / 2.0
    }

    pub fn solve(&mut self, iters: usize) {
        let mut history: SmallVec<[Action; MAX_ACTION_LENGTH]> = SmallVec::new();

        let target = self.iters + iters;
        while self.iters < target {
            if self.iters % 1000 == 0 {
                println!("info cfr iter {}", self.iters);
            }
            let board = Board::new();
            let reach_prob = [1.0; Player::COUNT];

            let board_groups = make_board_groups(&[board.clone()], &self.street_groups)
                .values()
                .next()
                .cloned()
                .unwrap();
            self.cfr(&reach_prob, &board, &mut history, &board_groups);

            if self.iters % 10000 == 0 {
                println!(
                    "info cfr exploitability {:.5} sb",
                    self.exploitability(5000)
                );
            }

            self.iters += 1;
        }
    }
}
