use rand::{SeedableRng, rngs::StdRng};
use rustyline::DefaultEditor;

use crate::{game::board::Street, group::equity::Equity, solver::engine::Solver};

mod bits;
mod game;
mod group;
mod solver;

fn compute_equity_group(
    filename: &str,
    street: Street,
    samples: usize,
    num_centroids: usize,
    bins: usize,
) {
    let mut seed = [0; 32];
    seed[0] = 42;
    let mut rng = StdRng::from_seed(seed);

    println!(
        "info computing_equity_group with street {} samples {samples} centroids {num_centroids} bins {bins} filename {filename}",
        street.to_string()
    );

    let group = Equity::compute(samples, num_centroids, bins, street, &mut rng);
    group.save(filename);
    group.visualize(&mut rng, 100);
}

fn cfr_solve(street_groups: &[&str], iters: usize) {
    let mut solver = Solver::new(|street| {
        let filename = street_groups[street as usize];
        println!("info street {} filename {}", street.to_string(), filename);
        Box::new(Equity::load(filename))
    });
    solver.solve(iters);
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
                    // street centroids bins samples filename
                    let street: Street = parts[1].parse().unwrap();
                    let centroids: usize = parts[2].parse().unwrap();
                    let bins: usize = parts[3].parse().unwrap();
                    let samples: usize = parts[4].parse().unwrap();
                    let filename: &str = parts[5];
                    compute_equity_group(filename, street, samples, centroids, bins);
                }
                _ => {
                    println!("warn unknown group command {}", parts[0]);
                }
            },
            "cfr" => match parts[0] {
                "solve" => {
                    let iters: usize = parts[1].parse().unwrap();
                    let street_groups: Vec<&str> = Street::ALL
                        .into_iter()
                        .map(|street| parts[2 + street as usize])
                        .collect();
                    cfr_solve(&street_groups, iters);
                }
                _ => {
                    println!("warn unknown cfr command {}", parts[0]);
                }
            },
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
