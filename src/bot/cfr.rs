use rand::distr::Distribution;

use crate::{
    bot::bot::Bot,
    game::board::{Action, Board},
    solver::{engine::Solver, inference::InferenceSolver},
};

pub enum CfrSolver {
    Train(Solver),
    Inference(InferenceSolver),
}

pub struct Cfr {
    solver: CfrSolver,
}

impl Cfr {
    pub fn new(solver: CfrSolver) -> Self {
        Self { solver }
    }
}

impl Bot for Cfr {
    fn play(&mut self, board: &Board, history: &[Action]) -> Action {
        let actions = board.get_actions();
        let strategy = match &self.solver {
            CfrSolver::Train(solver) => &solver.lookup(board, history)[..actions.len()],
            CfrSolver::Inference(inference_solver) => {
                &inference_solver.lookup(board, history)[..actions.len()]
            }
        };
        let mut rng = rand::rng();
        let index = rand::distr::weighted::WeightedIndex::new(strategy)
            .unwrap()
            .sample(&mut rng);

        actions[index]
    }
}
