use rand::{Rng, RngExt};

use crate::{
    bot::bot::Bot,
    game::board::{Board, Outcome, Player},
};

pub struct Arena {
    players: [Box<dyn Bot>; Player::COUNT],
}

impl Arena {
    pub fn new(players: [Box<dyn Bot>; Player::COUNT]) -> Self {
        Self { players }
    }

    fn matchup(&mut self, rng: &mut impl Rng) -> [i32; Player::COUNT] {
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

            history.push(action);
            board.play(action);
        }

        let Outcome::Winnings(winnings) = board.outcome() else {
            unreachable!()
        };

        [
            winnings[p0_stm as usize] as i32,
            winnings[1 - p0_stm as usize] as i32,
        ]
    }

    pub fn play(&mut self, iters: usize) {
        let mut rng = rand::rng();

        let mut payouts = [0_i64; Player::COUNT];
        for iter in 0..iters {
            let winnings = self.matchup(&mut rng);
            for i in 0..Player::COUNT {
                payouts[i] += i64::from(winnings[i]);
            }

            let completed = iter + 1;
            if completed % 1000 == 0 || completed == iters {
                println!(
                    "info iter {completed} average_sb {:?}",
                    payouts[0] as f64 / completed as f64
                );
            }
        }
    }
}
