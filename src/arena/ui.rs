use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    str::FromStr,
};

use crate::{
    arena::pairwise::PairwiseStats,
    game::board::{Action, Board, Outcome, Player},
};

pub struct SolverProgram {
    name: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    loaded: bool,
}

impl SolverProgram {
    pub fn new(binary: &str) -> Self {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());

        Self {
            name: binary.to_string(),
            child,
            stdin,
            stdout,
            loaded: false,
        }
    }

    fn send(&mut self, line: &str) {
        writeln!(self.stdin, "{}", line).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Reads lines until one starts with `prefix`, returning the rest of that line.
    fn wait_for(&mut self, prefix: &str) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.stdout.read_line(&mut line).unwrap();
            if read == 0 {
                panic!(
                    "solver program exited while waiting for '{}' (status {:?})",
                    prefix,
                    self.child.try_wait()
                );
            }

            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                return rest.trim().to_string();
            }
            if trimmed.starts_with("warn") {
                panic!("solver program: {}", trimmed);
            }
        }
    }

    pub fn load(&mut self, path: &str, filename: &str) {
        self.send(&format!("play load {} {}", path, filename));
        self.loaded = true;
    }

    pub fn newgame(&mut self) {
        self.send("play newgame");
    }

    pub fn play(&mut self, board: &Board, history: &[Action]) -> Action {
        // format: play go side street hole_cards community_cards actions
        let cards = board.card_state();
        let stm = board.stm();

        let mut parts = vec![
            "play".to_string(),
            "go".to_string(),
            stm.to_string(),
            board.street().to_string(),
        ];
        parts.extend(
            cards.hands[stm as usize]
                .iter()
                .map(|card| card.to_string()),
        );
        parts.extend(cards.community.iter().map(|card| card.to_string()));
        parts.extend(history.iter().map(|action| action.to_string()));

        self.send(&parts.join(" "));

        let reply = self.wait_for("info bestmove");
        Action::from_str(&reply).unwrap()
    }
}

pub struct ProgramArena {
    programs: Vec<SolverProgram>,
    stats: PairwiseStats,
}

impl ProgramArena {
    pub fn new(programs: Vec<SolverProgram>) -> Self {
        let stats = PairwiseStats::new(programs.len());
        Self { programs, stats }
    }

    fn summary(&self) {
        let m = self.stats.len();

        let deals = self.stats.total();
        println!("info arena deals {} hands {}", deals, deals * 2.0);

        // pairwise means: mu_ij ± 1.96 sqrt(s2_ij / n_ij)
        for i in 0..m {
            for j in (i + 1)..m {
                let ci = match self.stats.mean_variance(i, j) {
                    Some(v) => format!("{:.4}", 1.96 * v.sqrt()),
                    None => "?".to_string(),
                };
                println!(
                    "info arena pair {} {} mean {:+.4} +- {} sb n {}",
                    self.programs[i].name,
                    self.programs[j].name,
                    self.stats.mean(i, j),
                    ci,
                    self.stats.count(i, j)
                );
            }
        }

        let Some(ratings) = self.stats.fit() else {
            println!("warn arena rating fit unavailable (matchup graph not connected yet)");
            return;
        };

        let mut order: Vec<usize> = (0..m).collect();
        order.sort_by(|&a, &b| ratings.s[b].total_cmp(&ratings.s[a]));
        for (rank, &i) in order.iter().enumerate() {
            println!(
                "info arena rank {} program {} rating {:+.4} +- {:.4} sb",
                rank + 1,
                self.programs[i].name,
                ratings.s[i],
                1.96 * ratings.se(i)
            );
        }

        // gaps between adjacent ranks
        for w in order.windows(2) {
            let (a, b) = (w[0], w[1]);
            println!(
                "info arena gap {} {} diff {:+.4} +- {:.4} sb",
                self.programs[a].name,
                self.programs[b].name,
                ratings.s[a] - ratings.s[b],
                1.96 * ratings.diff_se(a, b)
            );
        }

        if ratings.df > 0 {
            println!(
                "info arena fit chi2 {:.3} df {} ratio {:.3}",
                ratings.chi2,
                ratings.df,
                ratings.chi2 / ratings.df as f64
            );
        }
    }

    fn matchup(&mut self, mut board: Board, a: usize, b: usize) -> i16 {
        self.programs[a].newgame();
        self.programs[b].newgame();

        let mut history = vec![];

        while board.outcome() == Outcome::Ongoing {
            let action = if board.stm() == Player::BB {
                self.programs[a].play(&board, &history)
            } else {
                self.programs[b].play(&board, &history)
            };

            history.push(action);
            board.play(action);
        }

        let Outcome::Winnings(winnings) = board.outcome() else {
            unreachable!()
        };

        winnings[0]
    }

    pub fn play(&mut self, iters: usize) {
        for p in self.programs.iter() {
            assert!(p.loaded);
        }

        for iter in 0..iters {
            // round robin
            for i in 0..self.programs.len() {
                for j in (i + 1)..self.programs.len() {
                    let board = Board::new();
                    let for_i = self.matchup(board.clone(), i, j) as f64;
                    let for_j = self.matchup(board.clone(), j, i) as f64;

                    // duplicate sample: i's average over both seatings of the same deal
                    self.stats.record(i, j, (for_i - for_j) / 2.0);
                }
            }

            let completed = iter + 1;
            if completed % 1000 == 0 || completed == iters {
                self.summary();
            }
        }
    }
}
