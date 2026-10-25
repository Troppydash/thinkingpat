use rand::RngExt;

use crate::{
    bot::bot::Bot,
    game::board::{Action, Board},
};

pub struct Random;

impl Bot for Random {
    fn play(&mut self, board: &Board, _history: &[Action]) -> Action {
        let mut rng = rand::rng();
        let actions = board.get_actions();
        actions[rng.random_range(0..actions.len())]
    }
}
