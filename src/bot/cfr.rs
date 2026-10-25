use rand::distr::Distribution;

use crate::{
    bot::bot::Bot,
    game::board::{Action, Board},
    solver::engine::Solver,
};

pub struct Cfr {
    solver: Solver,
}

impl Cfr {
    pub fn new(solver: Solver) -> Self {
        Self { solver }
    }
}

impl Bot for Cfr {
    fn play(&mut self, board: &Board, history: &[Action]) -> Action {
        let actions = board.get_actions();
        let strategy = &self.solver.lookup(board, history)[..actions.len()];
        let mut rng = rand::rng();
        let index = rand::distr::weighted::WeightedIndex::new(strategy)
            .unwrap()
            .sample(&mut rng);

        actions[index]
    }
}
