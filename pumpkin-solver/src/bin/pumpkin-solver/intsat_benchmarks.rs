use std::io::Write;
mod flatzinc;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use clap::Parser;
use log::{info, warn, LevelFilter};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use pumpkin_solver::conflict_resolution::{IntSatConflictResolver, ResolutionResolver};
use pumpkin_solver::options::{CumulativeOptions, LearningOptions, RestartOptions, SolverOptions};
use pumpkin_solver::proof::ProofLog;
use pumpkin_solver::Solver;
use pumpkin_solver::statistics::configure_statistic_logging;

use crate::flatzinc::FlatZincOptions;

#[derive(Debug, Parser)]
#[command(arg_required_else_help = true)]
struct Args {
    #[clap(verbatim_doc_comment)]
    instance_path: PathBuf,

    #[arg(long = "use-intsat")]
    use_intsat: bool,

    #[arg(long = "skip-nogood-learning")]
    skip_nogood_learning: bool,

    #[arg(long = "verbose")]
    verbose: bool,

    #[arg(long = "time-limit")]
    time_limit: u64,
}

fn configure_logging_minizinc(stat_header: &'static str, verbose: bool, log_statistics: bool) -> std::io::Result<()> {
    if log_statistics {
        configure_statistic_logging(
            stat_header,
            None,
            None,
            None,
        );
    }

    let level_filter = if verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Warn
    };

    env_logger::Builder::new()
        .format(move |buf, record| {
            writeln!(buf, "{}", record.args())
        })
        .filter_level(level_filter)
        .target(env_logger::Target::Stdout)
        .init();

    info!("Logging successfully configured");
    Ok(())
}

static STAT_HEADER: OnceLock<String> = OnceLock::new();

fn main() {
    let args = Args::parse();

    let stat_header = STAT_HEADER.get_or_init(|| format!("$stat$-I{:?}-SL{:?}", args.use_intsat, args.skip_nogood_learning));
    let _ = configure_logging_minizinc(stat_header, args.verbose, true);

    if pumpkin_solver::asserts::PUMPKIN_ASSERT_LEVEL_DEFINITION
        >= pumpkin_solver::asserts::PUMPKIN_ASSERT_MODERATE
    {
        warn!("Potential performance degradation: the Pumpkin assert level is set to {}, meaning many debug asserts are active which may result in performance degradation.", pumpkin_solver::asserts::PUMPKIN_ASSERT_LEVEL_DEFINITION);
    };

    let solver_options = SolverOptions {
        restart_options: RestartOptions::default(),
        learning_clause_minimisation: true,
        random_generator: SmallRng::seed_from_u64(42),
        proof_log: ProofLog::default(),
        conflict_resolver: if args.use_intsat {
            Box::new(IntSatConflictResolver::new(args.skip_nogood_learning))
        } else {
            Box::new(ResolutionResolver::default())
        },
        learning_options: LearningOptions::default(),
    };

    let time_limit = Duration::from_millis(args.time_limit);
    let instance_path = args
        .instance_path
        .to_str()
        .expect("Invalid path");

    flatzinc::solve(
        Solver::with_options(solver_options),
        instance_path,
        Some(time_limit),
        FlatZincOptions {
            free_search: false,
            all_solutions: false,
            cumulative_options: CumulativeOptions::default(),
        },
    ).expect("Failed to solve");
}