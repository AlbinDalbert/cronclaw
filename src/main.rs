mod config;
mod pipeline;
mod runner;
mod state;

use clap::{Parser, Subcommand};
use fs2::FileExt;
use std::fs::{self, File};
use std::path::PathBuf;

fn cronclaw_home() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    PathBuf::from(home).join(".cronclaw")
}

#[derive(Parser)]
#[command(name = "cronclaw")]
#[command(about = "Cron-driven pipeline orchestrator for agents and programs")]
#[command(version)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise the cronclaw directory structure
    Init,
    /// Advance all pipelines by one tick
    Run,
    /// Reset a pipeline by removing its state file
    Reset {
        /// Name of the pipeline to reset
        pipeline: String,
    },
}

fn cmd_init() {
    let home = cronclaw_home();
    let pipelines_dir = home.join("pipelines");
    let config_path = home.join("config.yaml");

    if home.exists() {
        eprintln!("cronclaw directory already exists at {}", home.display());
        std::process::exit(1);
    }

    fs::create_dir_all(&pipelines_dir).expect("failed to create pipelines directory");

    fs::write(
        &config_path,
        "# cronclaw configuration\n# timeout: 300  # default step timeout in seconds\n",
    )
    .expect("failed to write config.yaml");

    println!("Initialised cronclaw at {}", home.display());
}

fn cmd_run(verbose: bool) {
    let home = cronclaw_home();
    if !home.exists() {
        eprintln!("cronclaw not initialised. Run `cronclaw init` first.");
        std::process::exit(1);
    }

    // Acquire exclusive lock — if another runner is active, exit immediately
    let lock_path = home.join("lock");
    let lock_file = File::create(&lock_path).expect("failed to create lock file");
    if lock_file.try_lock_exclusive().is_err() {
        if verbose {
            eprintln!("another cronclaw run is already in progress — exiting");
        }
        return;
    }
    // Lock held until lock_file is dropped at end of function

    let cfg = config::load(&home.join("config.yaml"));

    let pipelines_dir = home.join("pipelines");
    let entries = fs::read_dir(&pipelines_dir).expect("failed to read pipelines directory");

    let mut found = false;
    let mut errors = Vec::new();

    for entry in entries {
        let entry = entry.expect("failed to read directory entry");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let pipeline_file = path.join("pipeline.yaml");
        if !pipeline_file.exists() {
            continue;
        }

        found = true;

        if let Err(e) = runner::run_pipeline(&path, &cfg, verbose) {
            errors.push(e);
        }
    }

    if !found && verbose {
        println!("No pipelines found.");
    }

    if !errors.is_empty() {
        eprintln!();
        for e in &errors {
            eprintln!("error: {}", e);
        }
        std::process::exit(1);
    }
}

fn cmd_reset(pipeline: &str) {
    let home = cronclaw_home();
    let state_file = home.join("pipelines").join(pipeline).join("state.json");

    if !state_file.exists() {
        println!("No state file for pipeline '{}'. Nothing to reset.", pipeline);
        return;
    }

    fs::remove_file(&state_file).expect("failed to remove state file");
    println!("Reset pipeline '{}'.", pipeline);
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => cmd_init(),
        Some(Commands::Run) => cmd_run(cli.verbose),
        Some(Commands::Reset { pipeline }) => cmd_reset(&pipeline),
        None => {
            let _ = Cli::parse_from(["cronclaw", "--help"]);
        }
    }
}
