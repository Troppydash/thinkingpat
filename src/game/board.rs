use rand::{RngExt, seq::index};

use crate::game::card::Card;

pub const PRESAMPLED_SIZE: usize = 2 + 2 + 5;
pub const CASH_IN_SB: i32 = 1000;
pub const BB_IN_SB: i32 = 2;

fn presample() -> [Card; PRESAMPLED_SIZE] {
    let mut rng = rand::rng();
    let mut cards = [Card::DEFAULT; PRESAMPLED_SIZE];
    let mut i = 0;
    for idx in index::sample(&mut rng, Card::COUNT, PRESAMPLED_SIZE) {
        cards[i] = Card::ALL[idx];
        i += 1;
    }

    cards
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Player {
    BB,
    SB,
}

impl Player {
    pub const COUNT: usize = 2;
}

pub struct CardState {
    hands: [Vec<Card>; Player::COUNT],
    visible_cards: Vec<Card>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Fold,
    Call,
    Raise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Ongoing,
    Winnings([i32; Player::COUNT]),
}

pub struct Board {
    presampled: [Card; PRESAMPLED_SIZE],
    street: Street,
    stm: Player,
    cash_in_sb: [i32; Player::COUNT],
    pot_in_sb: i32,
    raises_this_street: [i32; Player::COUNT],
    has_called_this_street: [bool; Player::COUNT],
    outcome: Outcome,
}

impl Board {
    // TODO: rules and randomness
    pub fn new() -> Self {
        let mut cash_in_sb = [CASH_IN_SB; Player::COUNT];
        let mut pot_in_sb = 0;

        // initial bets
        cash_in_sb[Player::SB as usize] -= 1;
        pot_in_sb += 1;
        cash_in_sb[Player::BB as usize] -= BB_IN_SB;
        pot_in_sb += BB_IN_SB;

        Self {
            presampled: presample(),
            street: Street::Preflop,
            stm: Player::SB,
            cash_in_sb,
            pot_in_sb,
            raises_this_street: [0; Player::COUNT],
            has_called_this_street: [false; Player::COUNT],
            outcome: Outcome::Ongoing,
        }
    }

    pub fn card_state(&self) -> CardState {
        let visible_end = 4 + match self.street {
            Street::Preflop => 0,
            Street::Flop => 3,
            Street::Turn => 4,
            Street::River => 5,
        };
        CardState {
            hands: [
                self.presampled[0..2].to_owned(),
                self.presampled[2..4].to_owned(),
            ],
            visible_cards: self.presampled[4..visible_end].to_owned(),
        }
    }

    fn bet(&mut self, player: Player, amount: i32) {
        self.cash_in_sb[player as usize] -= amount;
        self.pot_in_sb += amount;
    }

    pub fn get_actions(&self) -> Vec<Action> {}

    pub fn play(&mut self, action: Action) {}

    pub fn outcome(&self) -> Outcome {
        self.outcome
    }
}
