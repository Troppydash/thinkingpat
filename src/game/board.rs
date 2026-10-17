use std::ops::{Neg, Not};

use rand::{RngExt, seq::index};

use crate::game::{card::Card, showdown};

pub const PRESAMPLED_SIZE: usize = 2 + 2 + 5;
pub const CASH_IN_SB: i32 = 1000;
pub const BB_IN_SB: i32 = 2;
pub const MAX_RAISES_PER_STREET: i32 = 4;

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

fn make_oppsite<T>(score: T, side: Player) -> [T; Player::COUNT]
where
    T: Neg<Output = T> + Copy,
{
    let mut output = [score; Player::COUNT];
    output[!side as usize] = -score;
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

impl Street {
    fn next_street(&self) -> Option<Street> {
        match self {
            Street::Preflop => Some(Street::Flop),
            Street::Flop => Some(Street::Turn),
            Street::Turn => Some(Street::River),
            Street::River => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Player {
    BB,
    SB,
}

impl Player {
    pub const COUNT: usize = 2;
}

impl Not for Player {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::BB => Self::SB,
            Self::SB => Self::BB,
        }
    }
}

pub struct CardState {
    hands: [Vec<Card>; Player::COUNT],
    visible_cards: Vec<Card>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Fold,
    Call,
    Raise(i32),
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
    num_raises_this_street: [i32; Player::COUNT],
    raises_this_street_in_sb: [i32; Player::COUNT],
    has_called_this_street: [bool; Player::COUNT],
    outcome: Outcome,
}

impl Board {
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
            num_raises_this_street: [0; Player::COUNT],
            raises_this_street_in_sb: [0; Player::COUNT],
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

    fn raise_amount(&self) -> i32 {
        match self.street {
            Street::Preflop | Street::Flop => BB_IN_SB,
            Street::Turn | Street::River => BB_IN_SB * 2,
        }
    }

    pub fn get_actions(&self) -> Vec<Action> {
        debug_assert!(matches!(self.outcome, Outcome::Ongoing));

        let mut actions = vec![Action::Fold, Action::Call];
        if self.num_raises_this_street[self.stm as usize] < MAX_RAISES_PER_STREET {
            let raise_amount = self.raise_amount();
            if self.cash_in_sb[self.stm as usize] >= raise_amount {
                actions.push(Action::Raise(raise_amount));
            }
        }

        actions
    }

    fn showdown(&self) -> Option<Player> {
        showdown::winner(
            &self.presampled[0..2],
            &self.presampled[2..4],
            &self.presampled[4..],
        )
    }

    pub fn play(&mut self, action: Action) {
        debug_assert!(matches!(self.outcome, Outcome::Ongoing));

        match action {
            Action::Fold => {
                let pnl_in_sb = CASH_IN_SB - self.cash_in_sb[self.stm as usize];

                // not stm gets the pnl here
                self.outcome = Outcome::Winnings(make_oppsite(pnl_in_sb, !self.stm))
            }
            Action::Call => {
                debug_assert!(!self.has_called_this_street[self.stm as usize]);
                self.has_called_this_street[self.stm as usize] = true;
                let bet_in_sb = self.raises_this_street_in_sb[!self.stm as usize]
                    - self.raises_this_street_in_sb[self.stm as usize];
                self.raises_this_street_in_sb[self.stm as usize] += bet_in_sb;
                self.pot_in_sb += bet_in_sb;
                self.cash_in_sb[self.stm as usize] -= bet_in_sb;

                if self.has_called_this_street[!self.stm as usize] {
                    // move to next street if done
                    let next_street = self.street.next_street();
                    match next_street {
                        Some(next_street) => {
                            self.street = next_street;

                            // reset some stuff
                            self.num_raises_this_street = [0; Player::COUNT];
                            self.has_called_this_street = [false; Player::COUNT];
                            self.raises_this_street_in_sb = [0; Player::COUNT];
                            self.stm = Player::SB;
                        }
                        None => {
                            // showdown
                            match self.showdown() {
                                None => {
                                    self.outcome = Outcome::Winnings([0, 0]);
                                }
                                Some(winner) => {
                                    self.outcome =
                                        Outcome::Winnings(make_oppsite(self.pot_in_sb / 2, winner));
                                }
                            }
                        }
                    }
                } else {
                    // switch
                    self.stm = !self.stm;
                }
            }
            Action::Raise(_) => todo!(),
        }
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome
    }
}
