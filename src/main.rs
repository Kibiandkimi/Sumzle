use clap::{Parser, Subcommand};
use sumzle_solver::constraints::build_constraints;
use sumzle_solver::distributed::{Coordinator, Worker};
use sumzle_solver::input::{interactive_input, parse_json_input};
use sumzle_solver::parallel::ParallelSolver;
use sumzle_solver::solver::{generate_prefixes, Solver};
use sumzle_solver::types::*;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sumzle-solver")]
#[command(about = "Sumzle puzzle solver with multi-threading and distributed support")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive mode: enter guesses interactively
    Interactive {
        /// Maximum solutions to find
        #[arg(short, long, default_value = "100")]
        max_solutions: usize,

        /// Number of threads (default: all cores)
        #[arg(short = 't', long)]
        threads: Option<usize>,

        /// Solving strategy: rayon, channel, steal
        #[arg(short, long, default_value = "rayon")]
        strategy: String,

        /// Write found solutions to file
        #[arg(short, long)]
        output_file: Option<PathBuf>,
    },

    /// JSON mode: read puzzle from JSON string
    Json {
        /// JSON input string
        #[arg(short, long, conflicts_with = "input_file")]
        input: Option<String>,

        /// JSON input file path
        #[arg(long, conflicts_with = "input")]
        input_file: Option<PathBuf>,

        /// Maximum solutions
        #[arg(short, long, default_value = "100")]
        max_solutions: usize,

        /// Number of threads
        #[arg(short = 't', long)]
        threads: Option<usize>,

        /// Solving strategy
        #[arg(short, long, default_value = "rayon")]
        strategy: String,

        /// Write found solutions to file
        #[arg(short, long)]
        output_file: Option<PathBuf>,
    },

    /// Single-threaded mode (for benchmarking)
    Single {
        /// JSON input string
        #[arg(short, long, conflicts_with = "input_file")]
        input: Option<String>,

        /// JSON input file path
        #[arg(long, conflicts_with = "input")]
        input_file: Option<PathBuf>,

        #[arg(short, long, default_value = "100")]
        max_solutions: usize,

        /// Write found solutions to file
        #[arg(short, long)]
        output_file: Option<PathBuf>,
    },

    /// Run as distributed coordinator
    Coordinator {
        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0:9876")]
        bind: String,

        /// JSON puzzle input string
        #[arg(short, long, conflicts_with = "input_file")]
        input: Option<String>,

        /// JSON puzzle input file path
        #[arg(long, conflicts_with = "input")]
        input_file: Option<PathBuf>,

        /// Max solutions
        #[arg(short, long, default_value = "100")]
        max_solutions: usize,

        /// Prefix length for task splitting
        #[arg(short, long, default_value = "3")]
        prefix_len: usize,

        /// Write found solutions to file
        #[arg(short, long)]
        output_file: Option<PathBuf>,
    },

    /// Run as distributed worker
    Worker {
        /// Coordinator address
        #[arg(short, long, default_value = "127.0.0.1:9876")]
        coordinator: String,

        /// Number of local threads
        #[arg(short = 't', long)]
        threads: Option<usize>,
    },

    /// Benchmark: compare strategies
    Benchmark {
        /// JSON input string
        #[arg(short, long, conflicts_with = "input_file")]
        input: Option<String>,

        /// JSON input file path
        #[arg(long, conflicts_with = "input")]
        input_file: Option<PathBuf>,

        #[arg(short, long, default_value = "50")]
        max_solutions: usize,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Interactive {
            max_solutions,
            threads,
            strategy,
            output_file,
        } => {
            let puzzle = interactive_input().expect("Failed to read input");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let solutions = run_parallel(&constraints, max_solutions, threads, &strategy);
            print_solutions(&solutions, output_file).expect("Failed to print/write solutions");
        }

        Commands::Json {
            input,
            input_file,
            max_solutions,
            threads,
            strategy,
            output_file,
        } => {
            let puzzle = load_json_puzzle(input, input_file).expect("Failed to read/parse JSON");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let solutions = run_parallel(&constraints, max_solutions, threads, &strategy);
            print_solutions(&solutions, output_file).expect("Failed to print/write solutions");
        }

        Commands::Single {
            input,
            input_file,
            max_solutions,
            output_file,
        } => {
            let puzzle = load_json_puzzle(input, input_file).expect("Failed to read/parse JSON");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let start = std::time::Instant::now();
            let mut solver = Solver::new(constraints, max_solutions);
            solver.solve();
            let elapsed = start.elapsed();

            println!("Single-threaded: found {} solutions in {:.2}s",
                     solver.solutions.len(), elapsed.as_secs_f64());
            print_solutions(&solver.solutions, output_file).expect("Failed to print/write solutions");
        }

        Commands::Coordinator {
            bind,
            input,
            input_file,
            max_solutions,
            prefix_len,
            output_file,
        } => {
            let puzzle = load_json_puzzle(input, input_file).expect("Failed to read/parse JSON");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let prefixes = generate_prefixes(&constraints, prefix_len);
            println!("Generated {} tasks with prefix_len={}", prefixes.len(), prefix_len);

            let rt = tokio::runtime::Runtime::new().unwrap();
            let coordinator = Coordinator::new(bind, &constraints, prefixes, max_solutions);
            let solutions = rt.block_on(coordinator.run());
            print_solutions(&solutions, output_file).expect("Failed to print/write solutions");
        }

        Commands::Worker {
            coordinator,
            threads,
        } => {
            let num_threads = threads.unwrap_or_else(num_cpus::get);
            let rt = tokio::runtime::Runtime::new().unwrap();
            let worker = Worker::new(coordinator, num_threads);
            rt.block_on(worker.run());
        }

        Commands::Benchmark {
            input,
            input_file,
            max_solutions,
        } => {
            let puzzle = load_json_puzzle(input, input_file).expect("Failed to read/parse JSON");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let num_threads = num_cpus::get();
            println!("Benchmarking with {} threads...\n", num_threads);

            // Single-threaded
            {
                let start = std::time::Instant::now();
                let mut solver = Solver::new(constraints.clone(), max_solutions);
                solver.solve();
                let elapsed = start.elapsed();
                println!(
                    "Single-thread:  {} solutions in {:.3}s",
                    solver.solutions.len(),
                    elapsed.as_secs_f64()
                );
            }

            // Rayon
            {
                let solver = ParallelSolver::new(constraints.clone(), max_solutions, Some(num_threads));
                let start = std::time::Instant::now();
                let solutions = solver.solve_rayon();
                let elapsed = start.elapsed();
                println!(
                    "Rayon:          {} solutions in {:.3}s",
                    solutions.len(),
                    elapsed.as_secs_f64()
                );
            }

            // Channel
            {
                let solver = ParallelSolver::new(constraints.clone(), max_solutions, Some(num_threads));
                let start = std::time::Instant::now();
                let solutions = solver.solve_channel();
                let elapsed = start.elapsed();
                println!(
                    "Channel:        {} solutions in {:.3}s",
                    solutions.len(),
                    elapsed.as_secs_f64()
                );
            }

            // Work-stealing
            {
                let solver = ParallelSolver::new(constraints.clone(), max_solutions, Some(num_threads));
                let start = std::time::Instant::now();
                let solutions = solver.solve_work_stealing();
                let elapsed = start.elapsed();
                println!(
                    "Work-stealing:  {} solutions in {:.3}s",
                    solutions.len(),
                    elapsed.as_secs_f64()
                );
            }
        }
    }
}

fn load_json_puzzle(input: Option<String>, input_file: Option<PathBuf>) -> Result<PuzzleInput, String> {
    match (input, input_file) {
        (Some(raw), None) => parse_json_input(&raw),
        (None, Some(path)) => {
            let content =
                fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
            parse_json_input(&content)
        }
        _ => Err("Specify exactly one of --input or --input-file".into()),
    }
}

fn run_parallel(
    constraints: &Constraints,
    max_solutions: usize,
    threads: Option<usize>,
    strategy: &str,
) -> Vec<String> {
    let solver = ParallelSolver::new(constraints.clone(), max_solutions, threads);
    match strategy {
        "rayon" => solver.solve_rayon(),
        "channel" => solver.solve_channel(),
        "steal" => solver.solve_work_stealing(),
        other => {
            eprintln!("Unknown strategy '{}', using rayon", other);
            solver.solve_rayon()
        }
    }
}

fn print_solutions(solutions: &[String], output_file: Option<PathBuf>) -> Result<(), String> {
    println!("\n=== Found {} solutions ===", solutions.len());
    for (i, sol) in solutions.iter().enumerate() {
        println!("  [{}] {}", i + 1, sol);
    }

    if let Some(path) = output_file {
        let mut out = String::new();
        out.push_str(&format!("=== Found {} solutions ===\n", solutions.len()));
        for (i, sol) in solutions.iter().enumerate() {
            out.push_str(&format!("[{}] {}\n", i + 1, sol));
        }
        fs::write(&path, out)
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
        println!("Solutions written to {}", path.display());
    }

    Ok(())
}
