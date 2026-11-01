use core::panic;

use rand::{SeedableRng, rngs::StdRng};
use rustyline::DefaultEditor;

use crate::{
    arena::Arena,
    bot::{
        cfr::Cfr,
        random::{Caller, Raiser, Random},
    },
    game::{board::Street, card::CardSet},
    group::equity::to_pdf,
    solver::engine::{Solver, create_groups},
};

mod arena;
mod bits;
mod bot;
mod dist;
mod game;
mod group;
mod solver;
mod ver;

fn visualize_groups(samples: usize, path: &str) {
    let groups = create_groups(path);
    let rng = &mut rand::rng();

    for street in Street::ALL {
        let group = &groups[street as usize];
        let mut table = vec![vec![]; group.size()];
        for _ in 0..samples {
            let (hand, community) = CardSet::sample2(rng, 2, street.visible_community());
            let group = group.map(hand, community);
            table[group].push((hand, community));
        }

        for (i, entries) in table.iter().enumerate() {
            println!("info group {} entries {}", i, entries.len());
        }

        for (i, entries) in table.iter().enumerate() {
            println!("info group {}", i);
            println!("info {:?}", to_pdf(&group.repr(i)));
            for (hand, community) in entries {
                println!("info {} | {}", hand.to_string(), community.to_string());
            }
        }
    }
}

fn cfr_solve(filename: &str, path: &str, iters: usize) {
    let mut solver = Solver::new(path);
    solver.solve(filename, iters);
}

fn test_random(filename: &str, path: &str, iters: usize) {
    let solver = Solver::load(filename, path);
    let cfr = Cfr::new(solver);
    let mut arena = Arena::new([Box::new(cfr), Box::new(Random)]);
    arena.play(iters);
}

fn test_equity(filename: &str, path: &str, iters: usize) {
    let solver = Solver::load(filename, path);
    let cfr = Cfr::new(solver);
    let mut arena = Arena::new([Box::new(cfr), Box::new(bot::equity::Equity)]);
    arena.play(iters);
}

fn ui(args: Vec<String>) {
    let process_line = |line: &String| -> () {
        let parts: Vec<_> = line.split_whitespace().collect();
        let command = parts[0];
        let parts = &parts[1..];

        match command {
            "hi" => {
                println!("info Hello");
            }
            "group" => match parts[0] {
                "equity" => {
                    let path: &str = parts[1];
                    let samples: usize = parts[2].parse().unwrap();
                    visualize_groups(samples, path);
                }
                _ => {
                    println!("warn unknown group command {}", parts[0]);
                }
            },
            "cfr" => match parts[0] {
                "solve" => {
                    let iters: usize = parts[1].parse().unwrap();
                    let filename = parts[2];
                    let path = parts[3];
                    cfr_solve(filename, path, iters);
                }
                _ => {
                    println!("warn unknown cfr command {}", parts[0]);
                }
            },
            // "play" => match parts[0] {
            //     "test0" => {
            //         let iters: usize = parts[1].parse().unwrap();
            //         let filename = parts[2];
            //         let street_groups: Vec<&str> = Street::ALL
            //             .into_iter()
            //             .map(|street| parts[3 + street as usize])
            //             .collect();
            //         test0(filename, &street_groups, iters);
            //     }
            //     "test1" => {
            //         let iters: usize = parts[1].parse().unwrap();
            //         let filename = parts[2];
            //         let street_groups: Vec<&str> = Street::ALL
            //             .into_iter()
            //             .map(|street| parts[3 + street as usize])
            //             .collect();
            //         test1(filename, &street_groups, iters);
            //     }
            //     _ => {
            //         panic!("");
            //     }
            // },
            _ => {
                println!("warn unknown command {}", command);
            }
        }
    };

    for arg in args.iter().skip(1) {
        process_line(arg);
    }

    let mut rl = DefaultEditor::new().unwrap();
    loop {
        let line = rl.readline("");
        let line = match line {
            Err(_) => {
                break;
            }
            Ok(line) => line,
        };

        rl.add_history_entry(&line).unwrap();
        process_line(&line);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    ui(args);
}
