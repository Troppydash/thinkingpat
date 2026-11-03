use rand::{Rng, RngExt};

use crate::{
    bot::bot::Bot,
    game::{
        board::{Board, Outcome, Player},
        card::CardSet,
    },
};

pub struct Arena {
    players: [Box<dyn Bot>; Player::COUNT],
    verbose: bool,
}

impl Arena {
    pub fn new(players: [Box<dyn Bot>; Player::COUNT]) -> Self {
        Self {
            players,
            verbose: false,
        }
    }

    fn matchup(&mut self, rng: &mut impl Rng) -> [i32; Player::COUNT] {
        if self.verbose {
            println!();
        }

        for player in self.players.iter_mut() {
            player.reset();
        }

        let mut board = Board::new();
        let mut history = vec![];
        let p0_stm = Player::ALL[rng.random_range(0..Player::COUNT)];
        while board.outcome() == Outcome::Ongoing {
            let action = if board.stm() == p0_stm {
                self.players[0].play(&board, &history)
            } else {
                self.players[1].play(&board, &history)
            };

            if self.verbose {
                let cards = board.card_state_full();
                println!(
                    "info board {} | {} ",
                    cards.hands[board.stm() as usize].to_string(),
                    cards.community[0..board.street().visible_community()]
                        .iter()
                        .cloned()
                        .collect::<CardSet>()
                        .to_string()
                );
                println!(
                    "info player {} action {:?}",
                    if p0_stm == board.stm() { "p0" } else { "p1" },
                    action
                );
            }
            history.push(action);
            board.play(action);
        }

        let Outcome::Winnings(winnings) = board.outcome() else {
            unreachable!()
        };

        if self.verbose {
            println!("info p0 winnings {}", winnings[p0_stm as usize]);
        }

        [
            winnings[p0_stm as usize] as i32,
            winnings[1 - p0_stm as usize] as i32,
        ]
    }

    pub fn play(&mut self, iters: usize) {
        let mut rng = rand::rng();

        let mut mean_payout = 0.0;
        let mut m2_payout = 0.0;
        for iter in 0..iters {
            let winnings = self.matchup(&mut rng);

            let n = (iter + 1) as f64;
            let x = winnings[0] as f64;
            let delta = x - mean_payout;
            mean_payout = mean_payout + delta / n;
            m2_payout = m2_payout + delta * (x - mean_payout);

            let completed = iter + 1;
            if completed % 1000 == 0 || completed == iters {
                let est_pop_var = m2_payout / (n - 1.0).max(1.0);
                let sample_mean = (est_pop_var / n).sqrt();
                println!(
                    "info iter {completed} sb {} +- {}",
                    mean_payout,
                    sample_mean * 1.96
                );
            }
        }
    }
}
