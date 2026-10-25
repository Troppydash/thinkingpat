use crate::{
    bot::bot::Bot,
    game::{
        board::{Action, Board},
        card::CardSet,
    },
    group::equity::{simulate_equity, to_pdf},
};

pub struct Equity;

impl Bot for Equity {
    fn play(&mut self, board: &Board, _history: &[Action]) -> Action {
        let card = board.card_state();
        let hand = card.hands[board.stm() as usize];
        let community = CardSet::from_iter(
            card.community[..board.street().visible_community()]
                .iter()
                .cloned(),
        );
        let bins = 1000;
        let equity = to_pdf(&simulate_equity(hand, community, bins, 100, Some(100)));
        let equity: f32 = equity
            .iter()
            .enumerate()
            .map(|(i, x)| x * i as f32 / bins as f32)
            .sum();

        let pot = board.pot();
        let mut best = f32::NEG_INFINITY;
        let mut best_i = 0;
        for (i, action) in board.get_actions().iter().enumerate() {
            let ev = if *action == Action::Fold {
                0.0
            } else {
                equity * pot as f32 + (1.0 - equity) * -board.cost(*action) as f32
            };
            if ev > best {
                best_i = i;
                best = ev;
            }
        }

        board.get_actions()[best_i]
    }
}
