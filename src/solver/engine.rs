use fxhash::FxHashMap;
use smallvec::{SmallVec, smallvec};

use crate::{
    game::{
        board::{Action, Board, CardState, Outcome, Player, Street},
        card::CardSet,
    },
    group::{equity::Equity, group::Group},
};

pub const MAX_ACTION_LENGTH: usize = 4 * 4;
pub const MAX_ACTIONS: usize = 3;

#[derive(Hash, PartialEq, Eq, Clone)]
struct Infoset {
    street: Street,
    group: usize,
    history: SmallVec<[Action; MAX_ACTION_LENGTH]>,
}

fn get_or_default<'a>(
    table: &'a mut FxHashMap<Infoset, SmallVec<[f32; MAX_ACTIONS]>>,
    key: &Infoset,
    actions: usize,
) -> &'a mut SmallVec<[f32; MAX_ACTIONS]> {
    if !table.contains_key(key) {
        let value = smallvec![0.0; actions];
        table.insert(key.clone(), value);
    }

    table.get_mut(&key).unwrap()
}

pub struct Solver {
    regret_sum: FxHashMap<Infoset, SmallVec<[f32; MAX_ACTIONS]>>,
    strategy_sum: FxHashMap<Infoset, SmallVec<[f32; MAX_ACTIONS]>>,
    street_groups: [Box<dyn Group>; Street::COUNT],
}

impl Solver {
    pub fn new(load: impl Fn(Street) -> Box<dyn Group>) -> Self {
        Self {
            regret_sum: FxHashMap::default(),
            strategy_sum: FxHashMap::default(),
            street_groups: Street::ALL.map(load),
        }
    }

    fn regret_matching(
        &mut self,
        infoset: &Infoset,
        actions: usize,
    ) -> SmallVec<[f32; MAX_ACTIONS]> {
        let regrets = get_or_default(&mut self.regret_sum, infoset, actions);
        let total: f32 = regrets.iter().map(|x| x.max(0.0)).sum();

        regrets.iter().map(|x| x / total).collect()
    }

    /// return utility
    fn cfr(
        &mut self,
        // optimizing player
        player: Player,
        reach_prob: &[f32; Player::COUNT],
        board: &Board,
        history: &mut SmallVec<[Action; MAX_ACTION_LENGTH]>,
    ) -> f32 {
        // terminal
        if let Outcome::Winnings(winnings) = board.outcome() {
            return winnings[player as usize] as f32;
        }

        let card_state = board.card_state();
        let group = &self.street_groups[board.street() as usize];
        let community_set = CardSet::from_iter(card_state.community.iter().cloned());
        let infoset = Infoset {
            street: board.street(),
            group: group.map(card_state.hands[board.stm() as usize], community_set),
            history: history.clone(),
        };

        let actions = board.get_actions();
        let strategy = self.regret_matching(&infoset, actions.len());

        let mut counterfactual_value: SmallVec<[f32; MAX_ACTIONS]> = smallvec![0.0; actions.len()];
        for (i, action) in actions.iter().enumerate() {
            let mut new_board = board.clone();
            new_board.play(*action);

            history.push(*action);

            let mut reach_prob = reach_prob.clone();
            reach_prob[board.stm() as usize] *= strategy[i];

            counterfactual_value[i] = self.cfr(player, &reach_prob, &new_board, history);
            history.pop();
        }

        let value: f32 = counterfactual_value
            .iter()
            .enumerate()
            .map(|(i, v)| strategy[i] * v)
            .sum();

        // regret sum update
        let regrets = get_or_default(&mut self.regret_sum, &infoset, actions.len());
        let strategies = get_or_default(&mut self.strategy_sum, &infoset, actions.len());
        for i in 0..actions.len() {
            let opp_reach_prob = reach_prob[!player as usize];
            regrets[i] += opp_reach_prob * (value - counterfactual_value[i]);
            strategies[i] += reach_prob[player as usize] * strategy[i];
        }

        return value;
    }

    pub fn solve(&mut self, iters: usize) {
        let mut history: SmallVec<[Action; MAX_ACTION_LENGTH]> = SmallVec::new();
        for iter in 0..iters {
            println!("info cfr iter {}", iter);
            let board = Board::new();
            for p in Player::ALL {
                let reach_prob = [1.0; Player::COUNT];
                self.cfr(p, &reach_prob, &board, &mut history);
            }
        }
    }
}
