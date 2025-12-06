//! コマンドラインインターフェース

use crate::config;

/// コマンドライン引数
#[derive(Debug, Clone)]
pub struct Args {
    pub command: Command,
    pub method: String,
    pub max_iter: usize,
    pub threads: usize,
    pub verbose: bool,
    pub quiet: bool,
}

#[derive(Debug, Clone)]
pub enum Command {
    Solve { file: String },
    Bench { dim: usize },
    Repl,
    Version,
    Help,
}

impl Args {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        if args.is_empty() {
            return Ok(Args {
                command: Command::Help,
                method: "auto".to_string(),
                max_iter: config::DEFAULT_MAX_ITER,
                threads: num_cpus(),
                verbose: false,
                quiet: false,
            });
        }

        let cmd_str = &args[0];
        let command = match cmd_str.as_str() {
            "solve" => {
                if args.len() < 2 {
                    return Err("error: no input file".to_string());
                }
                Command::Solve {
                    file: args[1].clone(),
                }
            }
            "bench" => {
                let dim = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
                Command::Bench { dim }
            }
            "repl" => Command::Repl,
            "version" | "-v" | "--version" => Command::Version,
            "help" | "-h" | "--help" => Command::Help,
            _ => Command::Solve {
                file: cmd_str.clone(),
            },
        };

        let mut method = "auto".to_string();
        let mut max_iter = config::DEFAULT_MAX_ITER;
        let mut threads = num_cpus();
        let mut verbose = false;
        let mut quiet = false;

        let start_idx = match command {
            Command::Solve { .. } if !args[0].starts_with('-') => 2,
            _ => 1,
        };

        let mut i = start_idx;
        while i < args.len() {
            match args[i].as_str() {
                "-m" | "--method" => {
                    method = args
                        .get(i + 1)
                        .cloned()
                        .unwrap_or_else(|| "auto".to_string());
                    i += 1;
                }
                "-i" | "--iter" => {
                    max_iter = args
                        .get(i + 1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(config::DEFAULT_MAX_ITER);
                    i += 1;
                }
                "-t" | "--threads" => {
                    threads = args
                        .get(i + 1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(num_cpus());
                    i += 1;
                }
                "-v" | "--verbose" => verbose = true,
                "-q" | "--quiet" => quiet = true,
                _ => {}
            }
            i += 1;
        }

        Ok(Args {
            command,
            method,
            max_iter,
            threads,
            verbose,
            quiet,
        })
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}
