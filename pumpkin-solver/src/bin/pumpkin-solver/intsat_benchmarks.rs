use std::error::Error;
use std::fs::OpenOptions;
use std::io::stdout;
use std::io::Write;
mod flatzinc;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::time::Duration;

use clap::Parser;
use log::warn;
use log::LevelFilter;
use pumpkin_solver::options::ConflictResolver::IntSat;
use pumpkin_solver::options::ConflictResolver::UIP;
use pumpkin_solver::options::CumulativeOptions;
use pumpkin_solver::options::LearningOptions;
use pumpkin_solver::options::RestartOptions;
use pumpkin_solver::options::SolverOptions;
use pumpkin_solver::proof::ProofLog;
use pumpkin_solver::statistics::configure_statistic_logging;
use pumpkin_solver::Solver;
use rand::rngs::SmallRng;
use rand::SeedableRng;

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

    #[arg(long = "all-solutions")]
    all_solutions: bool,

    #[arg(long = "verbose")]
    verbose: bool,

    #[arg(long = "log-to-files")]
    log_to_files: bool,

    #[arg(long = "time-limit")]
    time_limit: Option<u64>,
}

static STAT_HEADER: OnceLock<String> = OnceLock::new();

fn open_file(name: &str) -> Box<dyn Write + Send + Sync> {
    let f = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(name)
        .expect("Cannot open file");

    Box::new(f)
}

static OUTPUT_LOGGER: OnceLock<RwLock<Box<dyn Write + Send + Sync>>> = OnceLock::new();

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut general_logger = if args.log_to_files {
        open_file("run_info")
    } else {
        Box::new(stdout())
    };
    let stats_logger = if args.log_to_files {
        open_file("run_stats")
    } else {
        Box::new(stdout())
    };
    let output_logger = OUTPUT_LOGGER.get_or_init(|| {
        RwLock::from(if args.log_to_files {
            open_file("run_outputs")
        } else {
            Box::new(stdout())
        })
    });

    writeln!(&mut general_logger, "Version: 6")?;
    writeln!(&mut general_logger, "File: {:?}", args.instance_path)?;
    writeln!(&mut general_logger, "All solutions: {:?}", args.all_solutions)?;
    writeln!(&mut general_logger, "Time-limit: {:?}", args.time_limit)?;
    writeln!(&mut general_logger, "Use intsat: {:?}", args.use_intsat)?;
    writeln!(&mut general_logger, "Skip nogood learning: {:?}", args.skip_nogood_learning)?;

    let stat_header = STAT_HEADER.get_or_init(|| {
        format!(
            "$stat$-I{:?}-SL{:?}",
            args.use_intsat, args.skip_nogood_learning
        )
    });

    // Configure logging
    configure_statistic_logging(stat_header, None, None, Some(stats_logger));

    let level_filter = if args.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Warn
    };

    env_logger::Builder::new()
        .format(move |buf, record| writeln!(buf, "{}", record.args()))
        .filter_level(level_filter)
        .target(env_logger::Target::Stdout)
        .init();

    if pumpkin_solver::asserts::PUMPKIN_ASSERT_LEVEL_DEFINITION
        >= pumpkin_solver::asserts::PUMPKIN_ASSERT_MODERATE
    {
        warn!("Potential performance degradation: the Pumpkin assert level is set to {}, meaning many debug asserts are active which may result in performance degradation.", pumpkin_solver::asserts::PUMPKIN_ASSERT_LEVEL_DEFINITION);
    };

    let mut learning_options = LearningOptions::default();
    learning_options.skip_nogood_learning = args.skip_nogood_learning;

    let solver_options = SolverOptions {
        restart_options: RestartOptions::default(),
        learning_clause_minimisation: true,
        random_generator: SmallRng::seed_from_u64(42),
        proof_log: ProofLog::default(),
        conflict_resolver: if args.use_intsat { IntSat } else { UIP },
        learning_options,
    };

    let time_limit = args.time_limit.map(Duration::from_millis);

    let instance_path = args.instance_path.to_str().expect("Invalid path");

    flatzinc::solve(
        Solver::with_options(solver_options),
        instance_path,
        time_limit,
        FlatZincOptions {
            free_search: false,
            all_solutions: args.all_solutions,
            cumulative_options: CumulativeOptions::default(),
        },
        output_logger,
    )?;

    Ok(())
}
