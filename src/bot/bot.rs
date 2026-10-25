use crate::game::board::{Action, Board};

pub trait Bot {
    fn reset(&mut self) {}
    fn play(&mut self, board: &Board, history: &[Action]) -> Action;
}
