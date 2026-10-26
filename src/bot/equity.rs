use crate::{
    bot::bot::Bot,
    game::board::{Action, Board},
    group::equity::{simulate_equity, to_pdf},
};

pub struct Equity;

impl Bot for Equity {
    fn play(&mut self, board: &Board, history: &[Action]) -> Action {
        let card = board.card_state();
        let hand = card.hands[board.stm() as usize];
        let community = card.community[..board.street().visible_community()]
            .iter()
            .cloned()
            .collect();
        let bins = 1000;
        let equity = to_pdf(&simulate_equity(hand, community, bins, 100, Some(100)));
        let equity: f32 = equity
            .iter()
            .enumerate()
            .map(|(i, x)| x * i as f32 / bins as f32)
            .sum();

        let raises = history
            .iter()
            .filter(|x| match *x {
                Action::Fold => false,
                Action::Call => false,
                Action::Raise(_) => true,
            })
            .cloned()
            .collect::<Vec<Action>>()
            .len();

        let fade = 0.01 * raises as f32;

        let pot = board.pot();
        let mut best = f32::NEG_INFINITY;
        let mut best_i = 0;
        for (i, action) in board.get_actions().iter().enumerate() {
            let ev = if *action == Action::Fold {
                0.0
            } else {
                // Assume the opponent calls our extra raise.
                let matching_bet = match *action {
                    Action::Raise(amount) => amount,
                    _ => 0,
                };
                equity * (pot + matching_bet) as f32
                    - (1.0 - equity) * board.cost(*action) as f32
                    - fade
            };
            if ev > best {
                best_i = i;
                best = ev;
            }
        }

        board.get_actions()[best_i]
    }
}
