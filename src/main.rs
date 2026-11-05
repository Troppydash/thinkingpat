#![cfg_attr(target_arch = "aarch64", feature(stdarch_aarch64_prefetch))]

use core::panic;

use rustyline::DefaultEditor;

use crate::{
    arena::Arena,
    bot::{cfr::Cfr, random::Random},
    game::{board::Street, card::CardSet},
    group::equity::to_pdf,
    solver::engine::{EngineGroups, EngineGroupsVersions, Solver},
    ver::try_load,
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
    let groups =
        try_load::<EngineGroupsVersions, EngineGroups>(&EngineGroupsVersions::V4, path).groups;
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
    let groups = try_load(&EngineGroupsVersions::V4, path);
    let mut solver = Solver::new(groups);
    solver.solve(filename, iters);
}

fn test_random(filename: &str, path: &str, iters: usize) {
    let groups = try_load(&EngineGroupsVersions::V4, path);
    let solver = Solver::load(filename, groups);
    let cfr = Cfr::new(solver);
    let mut arena = Arena::new([Box::new(cfr), Box::new(Random)]);
    arena.play(iters);
}

fn test_equity(filename: &str, path: &str, iters: usize) {
    let groups = try_load(&EngineGroupsVersions::V4, path);
    let solver = Solver::load(filename, groups);
    let cfr = Cfr::new(solver);
    let mut arena = Arena::new([Box::new(cfr), Box::new(bot::equity::Equity)]);
    arena.play(iters);
}

fn test_versions(filename_a: &str, filename_b: &str, path: &str, iters: usize) {
    let groups_a = try_load(&EngineGroupsVersions::V4, path);

    let solver_a = Solver::load(filename_a, groups_a);
    let cfr_a = Cfr::new(solver_a);

    let groups_b = try_load(&EngineGroupsVersions::V3, path);
    let solver_b = Solver::load(filename_b, groups_b);
    let cfr_b = Cfr::new(solver_b);

    let mut arena = Arena::new([Box::new(cfr_a), Box::new(cfr_b)]);
    arena.play(iters);
}

fn ui(args: Vec<String>) {
    let process_line = |line: &String| -> bool {
        let parts: Vec<_> = line.split_whitespace().collect();
        let command = parts[0];
        let parts = &parts[1..];

        match command {
            "hi" => {
                println!("info Hello");
            }
            "quit" => {
                return true;
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
                    let path = parts[1];
                    let filename = parts[2];
                    cfr_solve(filename, path, usize::MAX);
                }
                _ => {
                    println!("warn unknown cfr command {}", parts[0]);
                }
            },
            "play" => match parts[0] {
                "random" => {
                    let path = parts[1];
                    let filename = parts[2];
                    let iters: usize = parts[3].parse().unwrap();

                    test_random(filename, path, iters);
                }
                "equity" => {
                    let path = parts[1];
                    let filename = parts[2];
                    let iters: usize = parts[3].parse().unwrap();

                    test_equity(filename, path, iters);
                }
                "versions" => {
                    let path = parts[1];
                    let filename_a = parts[2];
                    let filename_b = parts[3];
                    let iters: usize = parts[4].parse().unwrap();
                    test_versions(filename_a, filename_b, path, iters);
                }
                _ => {
                    panic!("");
                }
            },
            _ => {
                println!("warn unknown command {}", command);
            }
        }

        false
    };

    for arg in args.iter().skip(1) {
        if process_line(arg) {
            return;
        }
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
        if process_line(&line) {
            break;
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    ui(args);
}
