use clap::{Parser, Subcommand};
use sumzle_solver::constraints::build_constraints;
use sumzle_solver::distributed::{Coordinator, Worker};
use sumzle_solver::input::{interactive_input, parse_json_input};
use sumzle_solver::parallel::ParallelSolver;
use sumzle_solver::solver::{generate_prefixes, Solver};
use sumzle_solver::types::*;

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
    },

    /// JSON mode: read puzzle from JSON string
    Json {
        /// JSON input string
        #[arg(short, long)]
        input: String,

        /// Maximum solutions
        #[arg(short, long, default_value = "100")]
        max_solutions: usize,

        /// Number of threads
        #[arg(short = 't', long)]
        threads: Option<usize>,

        /// Solving strategy
        #[arg(short, long, default_value = "rayon")]
        strategy: String,
    },

    /// Single-threaded mode (for benchmarking)
    Single {
        #[arg(short, long)]
        input: String,

        #[arg(short, long, default_value = "100")]
        max_solutions: usize,
    },

    /// Run as distributed coordinator
    Coordinator {
        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0:9876")]
        bind: String,

        /// JSON puzzle input
        #[arg(short, long)]
        input: String,

        /// Max solutions
        #[arg(short, long, default_value = "100")]
        max_solutions: usize,

        /// Prefix length for task splitting
        #[arg(short, long, default_value = "3")]
        prefix_len: usize,
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
        #[arg(short, long)]
        input: String,

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
        } => {
            let puzzle = interactive_input().expect("Failed to read input");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let solutions = run_parallel(&constraints, max_solutions, threads, &strategy);
            print_solutions(&solutions);
        }

        Commands::Json {
            input,
            max_solutions,
            threads,
            strategy,
        } => {
            let puzzle = parse_json_input(&input).expect("Failed to parse JSON");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let solutions = run_parallel(&constraints, max_solutions, threads, &strategy);
            print_solutions(&solutions);
        }

        Commands::Single {
            input,
            max_solutions,
        } => {
            let puzzle = parse_json_input(&input).expect("Failed to parse JSON");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let start = std::time::Instant::now();
            let mut solver = Solver::new(constraints, max_solutions);
            solver.solve();
            let elapsed = start.elapsed();

            println!("Single-threaded: found {} solutions in {:.2}s",
                     solver.solutions.len(), elapsed.as_secs_f64());
            print_solutions(&solver.solutions);
        }

        Commands::Coordinator {
            bind,
            input,
            max_solutions,
            prefix_len,
        } => {
            let puzzle = parse_json_input(&input).expect("Failed to parse JSON");
            let constraints = build_constraints(&puzzle).expect("Failed to build constraints");
            println!("{}", constraints);

            let prefixes = generate_prefixes(&constraints, prefix_len);
            println!("Generated {} tasks with prefix_len={}", prefixes.len(), prefix_len);

            let rt = tokio::runtime::Runtime::new().unwrap();
            let coordinator = Coordinator::new(bind, &constraints, prefixes, max_solutions);
            let solutions = rt.block_on(coordinator.run());
            print_solutions(&solutions);
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
            max_solutions,
        } => {
            let puzzle = parse_json_input(&input).expect("Failed to parse JSON");
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

fn print_solutions(solutions: &[String]) {
    println!("\n=== Found {} solutions ===", solutions.len());
    for (i, sol) in solutions.iter().enumerate() {
        println!("  [{}] {}", i + 1, sol);
    }
}
