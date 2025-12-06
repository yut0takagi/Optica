//! Optica - 超高速最適化DSL

mod cli;
mod config;
mod parser;
mod solver;

use std::fs;
use std::io::{self, BufRead, Write};
use std::time::Instant;

use cli::{Args, Command};
use config::*;
use parser::parse;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let args = match Args::parse(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    match &args.command {
        Command::Solve { file } => cmd_solve(file, &args),
        Command::Bench { dim } => cmd_bench(*dim, args.threads),
        Command::Repl => cmd_repl(),
        Command::Version => println!("optica {}", VERSION),
        Command::Help => print_help(),
    }
}

fn print_help() {
    println!(
        r#"optica - Ultra-fast Optimization DSL

USAGE:
    optica <file.optica> [OPTIONS]
    optica solve <file.optica> [OPTIONS]
    optica bench [DIM]
    optica repl

OPTIONS:
    -m, --method <METHOD>   de, pso, hybrid (default: auto)
    -i, --iter <N>          Max iterations (default: 1000)
    -t, --threads <N>       Threads (default: auto)
    -v, --verbose           Verbose output
    -q, --quiet             Quiet mode

EXAMPLES:
    optica model.optica
    optica solve model.optica -m de -i 2000
    optica bench 100"#
    );
}

fn cmd_solve(file: &str, args: &Args) {
    let source = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let start = Instant::now();

    // パース
    let mut model = match parse(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("parse error: {}", e);
            std::process::exit(1);
        }
    };
    // サイドカーJSON読み込み（同名 .json があれば取り込む）
    if let Some(json_path) = sidecar_json_path(file) {
        if json_path.exists() {
            if let Err(e) = parser::load_json_into(&mut model, &json_path) {
                eprintln!(
                    "warning: failed to load json {}: {}",
                    json_path.display(),
                    e
                );
            }
        }
    }

    if model.dim == 0 {
        eprintln!("error: no variables");
        std::process::exit(1);
    }

    if args.verbose {
        eprintln!(
            "[optica] dim={}, method={}, threads={}",
            model.dim, args.method, args.threads
        );
    }

    // CP制約があればCP-SATで解く
    let has_cp = !model.cp_globals.is_empty();
    let (best, fitness, iters) = if has_cp {
        if let Some(res) = crate::solver::solve_cp_entry(&model, args.max_iter, args.threads) {
            res
        } else {
            eprintln!("cp-sat unavailable; fallback to heuristic");
            solve_heuristic(&model, args)
        }
    } else {
        solve_heuristic(&model, args)
    };

    let elapsed = start.elapsed();
    let obj = if model.maximize { -fitness } else { fitness };

    if args.quiet {
        println!("{:.6e}", obj);
    } else {
        print_result(&model, &best, obj, fitness, iters, elapsed);
    }
}

fn sidecar_json_path(file: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(file);
    let stem = p.file_stem()?;
    let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut pb = parent.to_path_buf();
    pb.push(format!("{}.json", stem.to_string_lossy()));
    Some(pb)
}

fn solve_heuristic(model: &parser::Model, args: &Args) -> (Vec<f64>, f64, usize) {
    match args.method.as_str() {
        "de" => crate::solver::de(model, args.max_iter, args.threads),
        "pso" => crate::solver::pso(model, args.max_iter),
        "hybrid" => crate::solver::hybrid(model, args.max_iter, args.threads),
        _ => {
            if model.dim <= 20 {
                crate::solver::pso(model, args.max_iter)
            } else {
                crate::solver::de(model, args.max_iter, args.threads)
            }
        }
    }
}

fn print_result(
    model: &parser::Model,
    best: &[f64],
    obj: f64,
    fitness: f64,
    iters: usize,
    elapsed: std::time::Duration,
) {
    println!(
        "\nStatus: {}",
        if fitness.abs() < TOLERANCE {
            "optimal"
        } else {
            "feasible"
        }
    );
    println!("Objective: {:.6e}", obj);
    println!("Time: {:.3}s", elapsed.as_secs_f64());
    println!("Iterations: {}", iters);

    if !model.var_names.is_empty() {
        println!("\nVariables:");
        for (i, name) in model.var_names.iter().enumerate() {
            if best[i].abs() > DISPLAY_TOLERANCE {
                println!("  {} = {:.6}", name, best[i]);
            }
        }
    }
}

fn cmd_bench(dim: usize, threads: usize) {
    println!("Benchmark: dim={}, threads={}", dim, threads);
    println!("{}", "-".repeat(50));

    let lb: Vec<f64> = vec![-5.0; dim];
    let ub: Vec<f64> = vec![5.0; dim];
    let mut model = parser::Model::new();
    model.lb = lb.clone();
    model.ub = ub.clone();
    model.dim = dim;
    model.maximize = false;

    // ウォームアップ
    let _ = crate::solver::de(&model, 10, 1);

    // DE
    let start = Instant::now();
    let (_, f, _) = crate::solver::de(&model, 500, 1);
    let de_time = start.elapsed().as_secs_f64() * 1000.0;
    println!("DE:        {:>7.2}ms  f={:.2e}", de_time, f);

    // DE parallel
    let start = Instant::now();
    let (_, f, _) = crate::solver::de(&model, 500, threads);
    let de_par_time = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "DE({}T):   {:>7.2}ms  f={:.2e}  {:.1}x",
        threads,
        de_par_time,
        f,
        de_time / de_par_time
    );

    // PSO
    let start = Instant::now();
    let (_, f, _) = crate::solver::pso(&model, 500);
    let pso_time = start.elapsed().as_secs_f64() * 1000.0;
    println!("PSO:       {:>7.2}ms  f={:.2e}", pso_time, f);

    // Hybrid
    let start = Instant::now();
    let (_, f, _) = crate::solver::hybrid(&model, 500, threads);
    let hybrid_time = start.elapsed().as_secs_f64() * 1000.0;
    println!("Hybrid:    {:>7.2}ms  f={:.2e}", hybrid_time, f);

    println!("\nBest: DE({}T) = {:.2}ms", threads, de_par_time);
}

fn cmd_repl() {
    println!("optica {} REPL", VERSION);
    println!("Commands: solve, bench, quit");

    let stdin = io::stdin();
    loop {
        print!(">>> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            break;
        }

        let line = line.trim();
        match line {
            "quit" | "exit" => break,
            "bench" => cmd_bench(
                100,
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
            ),
            _ if line.starts_with("bench ") => {
                if let Ok(dim) = line[6..].trim().parse() {
                    cmd_bench(
                        dim,
                        std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(1),
                    );
                }
            }
            _ if line.starts_with("solve ") => {
                let file = line[6..].trim();
                let args = Args {
                    command: Command::Solve {
                        file: file.to_string(),
                    },
                    method: "auto".to_string(),
                    max_iter: DEFAULT_MAX_ITER,
                    threads: std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(1),
                    verbose: false,
                    quiet: false,
                };
                if let Command::Solve { file } = &args.command {
                    cmd_solve(file, &args);
                }
            }
            _ => println!("Unknown command: {}", line),
        }
    }
}
