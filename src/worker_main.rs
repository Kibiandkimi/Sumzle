use clap::Parser;
use sumzle_solver::distributed::Worker;

#[derive(Parser)]
#[command(name = "sumzle-worker")]
#[command(about = "Sumzle distributed worker node")]
struct Cli {
    /// Coordinator address
    #[arg(short, long, default_value = "127.0.0.1:9876")]
    coordinator: String,

    /// Number of local threads
    #[arg(short = 't', long)]
    threads: Option<usize>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    let num_threads = cli.threads.unwrap_or_else(num_cpus::get);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let worker = Worker::new(cli.coordinator, num_threads);
    rt.block_on(worker.run());
}
