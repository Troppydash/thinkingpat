use std::{
    ops::{Neg, Not},
    str::FromStr,
};

use rand::{rngs::ThreadRng, seq::index};
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};

use crate::game::{
    card::{Card, CardSet},
    showdown,
};

pub const PRESAMPLED_SIZE: usize = 2 + 2 + 5;
pub const CASH_IN_SB: i32 = 1000;
pub const BB_IN_SB: i32 = 2;
pub const MAX_RAISES_PER_STREET: i32 = 3;

fn make_oppsite<T>(score: T, side: Player) -> [T; Player::COUNT]
where
    T: Neg<Output = T> + Copy,
{
    let mut output = [score; Player::COUNT];
    output[!side as usize] = -score;
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
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

    pub fn visible_community(&self) -> usize {
        match self {
            Street::Preflop => 0,
            Street::Flop => 3,
            Street::Turn => 4,
            Street::River => 5,
        }
    }

    pub const COUNT: usize = 4;
    pub const ALL: [Street; Self::COUNT] = [Self::Preflop, Self::Flop, Self::Turn, Self::River];
}

#[derive(Debug)]
pub struct ParseStreetError;
impl FromStr for Street {
    type Err = ParseStreetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "preflop" => Ok(Self::Preflop),
            "flop" => Ok(Self::Flop),
            "turn" => Ok(Self::Turn),
            "river" => Ok(Self::River),
            _ => Err(ParseStreetError),
        }
    }
}

impl ToString for Street {
    fn to_string(&self) -> String {
        match self {
            Street::Preflop => "preflop".into(),
            Street::Flop => "flop".into(),
            Street::Turn => "turn".into(),
            Street::River => "river".into(),
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
    pub const ALL: [Player; Self::COUNT] = [Self::BB, Self::SB];
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

#[derive(Clone, Debug)]
pub struct CardState<Community = [Card; 5]> {
    pub hands: [CardSet; Player::COUNT],
    pub community: Community,
}

impl CardState {
    fn new() -> Self {
        // TODO: make this a seeded rng for reprod
        let mut rng = rand::rng();
        let p0 = CardSet::sample(&mut rng, 2);
        let p1 = CardSet::sample_remain(p0, &mut rng, 2);
        let community = CardSet::sample_remain_ordered::<5, ThreadRng>(p0 | p1, &mut rng);
        Self {
            hands: [p0, p1],
            community,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Fold,
    Call,
    // the i32 is EXTRA in additional to existing raises
    Raise(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Ongoing,
    Winnings([i32; Player::COUNT]),
}

#[derive(Clone, Debug)]
pub struct Board {
    presampled: CardState,
    street: Street,
    stm: Player,
    cash_in_sb: [i32; Player::COUNT],
    pot_in_sb: i32,
    num_raises_this_street: i32,
    raises_this_street_in_sb: [i32; Player::COUNT],
    has_called_this_street: [bool; Player::COUNT],
    outcome: Outcome,
}

impl Board {
    pub fn new() -> Self {
        let mut cash_in_sb = [CASH_IN_SB; Player::COUNT];
        let mut pot_in_sb = 0;
        let mut raises_this_street_in_sb = [0; Player::COUNT];

        // initial bets
        cash_in_sb[Player::SB as usize] -= 1;
        pot_in_sb += 1;
        raises_this_street_in_sb[Player::SB as usize] = 1;
        cash_in_sb[Player::BB as usize] -= BB_IN_SB;
        pot_in_sb += BB_IN_SB;
        raises_this_street_in_sb[Player::BB as usize] = BB_IN_SB;

        Self {
            presampled: CardState::new(),
            street: Street::Preflop,
            stm: Player::SB,
            cash_in_sb,
            pot_in_sb,
            num_raises_this_street: 0,
            raises_this_street_in_sb,
            has_called_this_street: [false; Player::COUNT],
            outcome: Outcome::Ongoing,
        }
    }

    pub fn card_state(&self) -> CardState<&[Card]> {
        CardState {
            hands: self.presampled.hands,
            community: &self.presampled.community[..self.street.visible_community()],
        }
    }

    pub fn street(&self) -> Street {
        self.street
    }

    pub fn stm(&self) -> Player {
        self.stm
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

    pub fn get_actions(&self) -> SmallVec<[Action; 3]> {
        debug_assert!(matches!(self.outcome, Outcome::Ongoing));

        let mut actions = smallvec![Action::Fold, Action::Call];
        if self.num_raises_this_street < MAX_RAISES_PER_STREET {
            let bet_in_sb = self.raise_amount() + self.raises_this_street_in_sb[!self.stm as usize]
                - self.raises_this_street_in_sb[self.stm as usize];
            if self.cash_in_sb[self.stm as usize] >= bet_in_sb {
                actions.push(Action::Raise(self.raise_amount()));
            }
        }

        actions
    }

    fn showdown(&self) -> Option<Player> {
        showdown::winner(
            self.presampled.hands[Player::BB as usize],
            self.presampled.hands[Player::SB as usize],
            self.presampled.community.iter().copied().collect(),
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
                debug_assert!(bet_in_sb >= 0);
                self.raises_this_street_in_sb[self.stm as usize] += bet_in_sb;
                self.pot_in_sb += bet_in_sb;
                self.cash_in_sb[self.stm as usize] -= bet_in_sb;

                if self.has_called_this_street[!self.stm as usize] {
                    // move to next street if done
                    let next_street = if self.cash_in_sb[self.stm as usize] == 0 {
                        None
                    } else {
                        self.street.next_street()
                    };
                    match next_street {
                        Some(next_street) => {
                            self.street = next_street;

                            // reset some stuff
                            self.num_raises_this_street = 0;
                            self.has_called_this_street = [false; Player::COUNT];
                            self.raises_this_street_in_sb = [0; Player::COUNT];
                            self.stm = Player::BB;
                        }
                        None => {
                            // showdown
                            self.street = Street::River;
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
            Action::Raise(amount) => {
                debug_assert!(
                    self.num_raises_this_street < MAX_RAISES_PER_STREET,
                    "raise cap reached"
                );
                // note that amount is excluding call amount
                let bet_in_sb = self.raises_this_street_in_sb[!self.stm as usize]
                    - self.raises_this_street_in_sb[self.stm as usize]
                    + amount;
                debug_assert!(bet_in_sb >= 0);

                self.num_raises_this_street += 1;
                self.has_called_this_street[self.stm as usize] = true;
                self.has_called_this_street[!self.stm as usize] = false;
                self.raises_this_street_in_sb[self.stm as usize] += bet_in_sb;
                self.pot_in_sb += bet_in_sb;
                self.cash_in_sb[self.stm as usize] -= bet_in_sb;
                self.stm = !self.stm;
            }
        }
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_card_state(text: &str) -> CardState {
        let cards = Card::parse_cards(text);
        assert_eq!(cards.len(), PRESAMPLED_SIZE);
        CardState {
            hands: [
                cards[..2].iter().copied().collect(),
                cards[2..4].iter().copied().collect(),
            ],
            community: cards[4..].try_into().unwrap(),
        }
    }

    fn street_board(street: Street) -> Board {
        if street == Street::Preflop {
            return Board::new();
        }

        let mut board = Board::new();
        board.street = street;
        board.stm = Player::BB;
        board.raises_this_street_in_sb = [0; Player::COUNT];
        board.cash_in_sb = [CASH_IN_SB - BB_IN_SB; Player::COUNT];
        board.pot_in_sb = BB_IN_SB * 2;
        board
    }

    fn assert_chips_conserved(board: &Board) {
        assert_eq!(
            board.cash_in_sb.iter().sum::<i32>() + board.pot_in_sb,
            CASH_IN_SB * Player::COUNT as i32,
        );
        assert!(board.cash_in_sb.iter().all(|&cash| cash >= 0));
    }

    #[test]
    fn new_board_posts_blinds_and_deals_distinct_cards() {
        let board = Board::new();
        assert_eq!(board.street, Street::Preflop);
        assert_eq!(board.stm, Player::SB);
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 2, CASH_IN_SB - 1]);
        assert_eq!(board.pot_in_sb, 3);
        assert_eq!(board.outcome(), Outcome::Ongoing);
        let [bb, sb] = board.presampled.hands;
        assert_eq!(bb.len(), 2);
        assert_eq!(sb.len(), 2);
        assert!(bb.is_disjoint(sb));
        let mut seen = bb | sb;
        for card in board.presampled.community {
            assert!(seen.insert(card), "duplicate dealt card: {card:?}");
        }
        assert_eq!(seen.len(), PRESAMPLED_SIZE);
        assert_chips_conserved(&board);
    }

    #[test]
    fn card_state_reveals_only_the_current_street() {
        let mut board = Board::new();
        for (street, count) in [
            (Street::Preflop, 0),
            (Street::Flop, 3),
            (Street::Turn, 4),
            (Street::River, 5),
        ] {
            board.street = street;
            let state = board.card_state();
            assert_eq!(state.hands, board.presampled.hands);
            assert_eq!(state.community.len(), count);
            assert_eq!(state.community, &board.presampled.community[..count]);
        }
    }

    #[test]
    fn small_blind_call_completes_the_big_blind() {
        let mut board = Board::new();
        board.play(Action::Call);
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 2; Player::COUNT]);
        assert_eq!(board.pot_in_sb, 4);
        assert_eq!(board.raises_this_street_in_sb, [2, 2]);
        assert_eq!(board.street, Street::Preflop);
        assert_eq!(board.stm, Player::BB);
        assert_chips_conserved(&board);
    }

    #[test]
    fn small_blind_raise_includes_the_blind_deficit() {
        let mut board = Board::new();
        board.play(Action::Raise(BB_IN_SB));
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 2, CASH_IN_SB - 4]);
        assert_eq!(board.pot_in_sb, 6);
        assert_eq!(board.raises_this_street_in_sb, [2, 4]);
        assert_eq!(board.stm, Player::BB);
        assert_chips_conserved(&board);
    }

    #[test]
    fn big_blind_can_raise_after_a_limp_and_small_blind_can_call() {
        let mut board = Board::new();
        board.play(Action::Call);
        board.play(Action::Raise(BB_IN_SB));
        board.play(Action::Call);
        assert_eq!(board.street, Street::Flop);
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 4; Player::COUNT]);
        assert_eq!(board.pot_in_sb, 8);
        assert_chips_conserved(&board);
    }

    #[test]
    fn big_blind_acts_first_on_every_postflop_street() {
        let mut board = Board::new();
        for street in [Street::Flop, Street::Turn, Street::River] {
            board.play(Action::Call);
            board.play(Action::Call);
            assert_eq!(board.street, street);
            assert_eq!(board.stm, Player::BB, "{street:?}");
        }
    }

    #[test]
    fn check_then_bet_allows_the_checker_to_call() {
        let mut board = street_board(Street::Flop);
        board.play(Action::Call);
        board.play(Action::Raise(BB_IN_SB));
        assert_eq!(board.street, Street::Flop);
        board.play(Action::Call);
        assert_eq!(board.street, Street::Turn);
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 4; Player::COUNT]);
        assert_eq!(board.pot_in_sb, 8);
        assert_chips_conserved(&board);
    }

    #[test]
    fn original_bettor_can_call_a_reraise() {
        let mut board = street_board(Street::Flop);
        board.play(Action::Raise(BB_IN_SB));
        board.play(Action::Raise(BB_IN_SB));
        assert_eq!(board.street, Street::Flop);
        board.play(Action::Call);
        assert_eq!(board.street, Street::Turn);
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 6; Player::COUNT]);
        assert_eq!(board.pot_in_sb, 12);
        assert_chips_conserved(&board);
    }

    #[test]
    fn bet_and_call_balance_contributions_and_reset_the_street() {
        let mut board = street_board(Street::Flop);
        board.play(Action::Raise(BB_IN_SB));
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 4, CASH_IN_SB - 2]);
        assert_eq!(board.pot_in_sb, 6);
        assert_eq!(board.stm, Player::SB);
        assert_eq!(board.outcome(), Outcome::Ongoing);
        board.play(Action::Call);
        assert_eq!(board.street, Street::Turn);
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 4; Player::COUNT]);
        assert_eq!(board.pot_in_sb, 8);
        assert_eq!(board.raises_this_street_in_sb, [0, 0]);
        assert_eq!(board.num_raises_this_street, 0);
        assert_eq!(board.has_called_this_street, [false, false]);
        assert_chips_conserved(&board);
    }

    #[test]
    fn fixed_limit_bets_double_on_turn_and_river() {
        for (street, amount) in [
            (Street::Preflop, 2),
            (Street::Flop, 2),
            (Street::Turn, 4),
            (Street::River, 4),
        ] {
            let board = street_board(street);
            assert_eq!(board.raise_amount(), amount);
            assert_eq!(
                board.get_actions(),
                SmallVec::from_buf([Action::Fold, Action::Call, Action::Raise(amount)]),
            );
        }
    }

    #[test]
    fn full_raise_requires_enough_cash_to_call_and_raise() {
        let mut board = street_board(Street::Flop);
        board.cash_in_sb = [2, 0];
        board.pot_in_sb = CASH_IN_SB * 2 - 2;
        board.raises_this_street_in_sb = [0, BB_IN_SB];
        assert!(!board.get_actions().contains(&Action::Raise(BB_IN_SB)));
        assert_chips_conserved(&board);
    }

    #[test]
    fn river_all_in_call_resolves_the_entire_pot() {
        let mut board = street_board(Street::River);
        board.presampled = parse_card_state("Ah Ad Kh Kd 2s 4c 7h 9d Js");
        board.cash_in_sb = [2, 0];
        board.pot_in_sb = CASH_IN_SB * 2 - 2;
        board.raises_this_street_in_sb = [0, BB_IN_SB];
        board.has_called_this_street = [false, true];
        board.play(Action::Call);
        assert_eq!(board.cash_in_sb, [0, 0]);
        assert_eq!(
            board.outcome(),
            Outcome::Winnings([CASH_IN_SB, -CASH_IN_SB])
        );
        assert_chips_conserved(&board);
    }

    #[test]
    fn folds_pay_the_opponents_net_profit() {
        let mut board = Board::new();
        board.play(Action::Fold);
        assert_eq!(board.outcome(), Outcome::Winnings([1, -1]));

        for player in [Player::BB, Player::SB] {
            let mut board = street_board(Street::Turn);
            board.stm = player;
            board.play(Action::Raise(4));
            board.play(Action::Fold);
            assert_eq!(board.outcome(), Outcome::Winnings(make_oppsite(2, player)));
            assert_chips_conserved(&board);
        }
    }

    #[test]
    fn river_calls_resolve_wins_and_split_pots() {
        for (cards, expected) in [
            ("Ah Ad Kh Kd 2s 4c 7h 9d Js", [10, -10]),
            ("Kh Kd Ah Ad 2s 4c 7h 9d Js", [-10, 10]),
            ("2h 3d 4h 5d As Ks Qs Js Ts", [0, 0]),
        ] {
            let mut board = street_board(Street::River);
            board.presampled = parse_card_state(cards);
            board.cash_in_sb = [CASH_IN_SB - 10; Player::COUNT];
            board.pot_in_sb = 20;
            board.play(Action::Call);
            assert_eq!(board.outcome(), Outcome::Ongoing);
            board.play(Action::Call);
            assert_eq!(board.outcome(), Outcome::Winnings(expected), "{cards}");
            assert_chips_conserved(&board);
        }
    }

    #[test]
    fn a_checked_down_hand_returns_the_losers_full_contribution() {
        let mut board = Board::new();
        board.presampled = parse_card_state("Ah Ad Kh Kd 2s 4c 7h 9d Js");
        for _ in 0..8 {
            board.play(Action::Call);
        }
        assert_eq!(board.outcome(), Outcome::Winnings([2, -2]));
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 2; Player::COUNT]);
        assert_eq!(board.pot_in_sb, 4);
        assert_chips_conserved(&board);
    }

    #[test]
    fn raise_cap_is_shared_by_both_players_on_every_street() {
        for street in [Street::Preflop, Street::Flop, Street::Turn, Street::River] {
            let mut board = street_board(street);
            let amount = board.raise_amount();
            for count in 1..=3 {
                assert!(board.get_actions().contains(&Action::Raise(amount)));
                board.play(Action::Raise(amount));
                assert_eq!(board.num_raises_this_street, count);
                assert_eq!(board.street, street);
                assert_chips_conserved(&board);
            }
            assert_eq!(
                board.get_actions(),
                SmallVec::from_buf([Action::Fold, Action::Call])
            );
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "raise cap reached")]
    fn playing_a_fourth_raise_is_rejected() {
        let mut board = Board::new();
        for _ in 0..4 {
            board.play(Action::Raise(BB_IN_SB));
        }
    }

    #[test]
    fn calling_the_last_raise_resets_the_cap_for_the_next_street() {
        let mut board = Board::new();
        for _ in 0..MAX_RAISES_PER_STREET {
            board.play(Action::Raise(BB_IN_SB));
        }
        board.play(Action::Call);
        assert_eq!(board.street, Street::Flop);
        assert_eq!(board.stm, Player::BB);
        assert_eq!(board.num_raises_this_street, 0);
        assert!(board.get_actions().contains(&Action::Raise(BB_IN_SB)));
        assert_eq!(board.cash_in_sb, [CASH_IN_SB - 8; Player::COUNT]);
        assert_eq!(board.pot_in_sb, 16);
        assert_chips_conserved(&board);
    }

    #[test]
    fn raises_across_all_streets_pay_the_correct_net_profit() {
        for (cards, expected) in [
            ("Ah Ad Kh Kd 2s 4c 7h 9d Js", [22, -22]),
            ("Kh Kd Ah Ad 2s 4c 7h 9d Js", [-22, 22]),
            ("2h 3d 4h 5d As Ks Qs Js Ts", [0, 0]),
        ] {
            let mut board = Board::new();
            board.presampled = parse_card_state(cards);
            for (street, actions) in [
                (
                    Street::Preflop,
                    vec![Action::Raise(2), Action::Raise(2), Action::Call],
                ),
                (
                    Street::Flop,
                    vec![
                        Action::Call,
                        Action::Raise(2),
                        Action::Raise(2),
                        Action::Call,
                    ],
                ),
                (Street::Turn, vec![Action::Raise(4), Action::Call]),
                (
                    Street::River,
                    vec![Action::Raise(4), Action::Raise(4), Action::Call],
                ),
            ] {
                for action in actions {
                    assert_eq!(board.street, street);
                    assert_eq!(board.outcome(), Outcome::Ongoing);
                    assert!(board.get_actions().contains(&action));
                    board.play(action);
                    assert_chips_conserved(&board);
                }
            }
            assert_eq!(board.cash_in_sb, [CASH_IN_SB - 22; Player::COUNT]);
            assert_eq!(board.pot_in_sb, 44);
            assert_eq!(board.outcome(), Outcome::Winnings(expected), "{cards}");
        }
    }

    #[test]
    fn matched_all_in_before_river_runs_out_the_board() {
        for street in [Street::Flop, Street::Turn] {
            let mut board = street_board(street);
            board.presampled = parse_card_state("Ah Ad Kh Kd 2s 4c 7h 9d Js");
            let amount = board.raise_amount();
            board.cash_in_sb = [amount; Player::COUNT];
            board.pot_in_sb = CASH_IN_SB * 2 - amount * 2;
            board.play(Action::Raise(amount));
            board.play(Action::Call);
            assert_eq!(board.cash_in_sb, [0, 0]);
            assert_chips_conserved(&board);
            assert_eq!(
                board.outcome(),
                Outcome::Winnings([CASH_IN_SB, -CASH_IN_SB]),
                "{street:?}",
            );
            assert_eq!(board.card_state().community.len(), 5);
        }
    }
}
