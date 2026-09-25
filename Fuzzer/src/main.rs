use std::path::PathBuf;
use std::str::FromStr;

use libafl::{
    corpus::{Corpus, CorpusId, InMemoryCorpus, OnDiskCorpus},
    events::SimpleEventManager,
    executors::{inprocess::InProcessExecutor, ExitKind},
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Evaluator, Fuzzer, StdFuzzer},
    inputs::{BytesInput, HasTargetBytes},
    monitors::SimpleMonitor,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::StdMapObserver,
    schedulers::{RandScheduler, Scheduler},
    stages::PowerMutationalStage,
    state::{HasCorpus, StdState},
    Error,
};
use libafl_bolts::{current_nanos, rands::StdRand, tuples::tuple_list};

mod scheduler;
use scheduler::{DirectedDistanceScheduler, EnergyScore, QueueScheduler, MAP_SIZE};

type TalonState =
    StdState<InMemoryCorpus<BytesInput>, BytesInput, StdRand, OnDiskCorpus<BytesInput>>;

extern "C" {
    fn target_function(data: *const u8, size: usize);
}

#[no_mangle]
static mut SIGNALS: [u8; MAP_SIZE] = [0; MAP_SIZE];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchedulerKind {
    Directed,
    Queue,
    Rand,
}

impl FromStr for SchedulerKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "directed" => Ok(Self::Directed),
            "queue" => Ok(Self::Queue),
            "rand" => Ok(Self::Rand),
            other => Err(format!(
                "unknown --scheduler kind '{other}' (expected directed, queue or rand)"
            )),
        }
    }
}

impl std::fmt::Display for SchedulerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Directed => "directed",
            Self::Queue => "queue",
            Self::Rand => "rand",
        })
    }
}

enum TalonScheduler {
    Directed(DirectedDistanceScheduler),
    Queue(QueueScheduler),
    Rand(RandScheduler<TalonState>),
}

impl Scheduler<BytesInput, TalonState> for TalonScheduler {
    fn on_add(&mut self, state: &mut TalonState, id: CorpusId) -> Result<(), Error> {
        match self {
            Self::Directed(s) => s.on_add(state, id),
            Self::Queue(s) => s.on_add(state, id),
            Self::Rand(s) => s.on_add(state, id),
        }
    }

    fn on_evaluation<OT>(
        &mut self,
        state: &mut TalonState,
        input: &BytesInput,
        observers: &OT,
    ) -> Result<(), Error>
    where
        OT: libafl_bolts::tuples::MatchName,
    {
        match self {
            Self::Directed(s) => s.on_evaluation(state, input, observers),
            Self::Queue(s) => s.on_evaluation(state, input, observers),
            Self::Rand(s) => s.on_evaluation(state, input, observers),
        }
    }

    fn next(&mut self, state: &mut TalonState) -> Result<CorpusId, Error> {
        match self {
            Self::Directed(s) => s.next(state),
            Self::Queue(s) => s.next(state),
            Self::Rand(s) => s.next(state),
        }
    }

    fn set_current_scheduled(
        &mut self,
        state: &mut TalonState,
        next_id: Option<CorpusId>,
    ) -> Result<(), Error> {
        match self {
            Self::Directed(s) => s.set_current_scheduled(state, next_id),
            Self::Queue(s) => s.set_current_scheduled(state, next_id),
            Self::Rand(s) => s.set_current_scheduled(state, next_id),
        }
    }
}

struct Args {
    distances: PathBuf,
    cooling_secs: f64,
    seed: u64,
    crashes_dir: PathBuf,
    scheduler: SchedulerKind,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            distances: PathBuf::from("distances.json"),
            cooling_secs: 5.0,
            seed: 0,
            crashes_dir: PathBuf::from("./crashes"),
            scheduler: SchedulerKind::Directed,
        }
    }
}

const USAGE: &str = "\
talon - directed greybox fuzzer

usage: dgf_core [flags]

  --distances PATH      distance map to load (default: distances.json)
  --cooling-secs SECS   annealing window in seconds (default: 5)
  --seed N              rng seed, printed so runs can be reproduced
  --crashes-dir DIR     where crash inputs are saved (default: ./crashes)
  --scheduler KIND      directed, queue or rand (default: directed)
  --help                print this message and exit";

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        seed: current_nanos(),
        ..Args::default()
    };

    let mut flags = std::env::args().skip(1);
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--distances" => {
                args.distances = flags.next().ok_or_else(|| missing_value(&flag))?.into();
            }
            "--cooling-secs" => {
                let value = flags.next().ok_or_else(|| missing_value(&flag))?;
                args.cooling_secs = value
                    .parse()
                    .map_err(|_| format!("--cooling-secs: '{value}' is not a number"))?;
            }
            "--seed" => {
                let value = flags.next().ok_or_else(|| missing_value(&flag))?;
                args.seed = value
                    .parse()
                    .map_err(|_| format!("--seed: '{value}' is not an integer"))?;
            }
            "--crashes-dir" => {
                args.crashes_dir = flags.next().ok_or_else(|| missing_value(&flag))?.into();
            }
            "--scheduler" => {
                let value = flags.next().ok_or_else(|| missing_value(&flag))?;
                args.scheduler = value.parse()?;
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(args)
}

fn missing_value(flag: &str) -> String {
    format!("{flag} needs a value")
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    println!("random seed: {} (pass --seed to reproduce)", args.seed);

    if let Err(e) = run(args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Error> {
    println!(
        "scheduler: {}, cooling window {}s",
        args.scheduler, args.cooling_secs
    );

    let scheduler = match args.scheduler {
        SchedulerKind::Directed => TalonScheduler::Directed(DirectedDistanceScheduler::new(
            &args.distances,
            args.cooling_secs,
        )?),
        SchedulerKind::Queue => TalonScheduler::Queue(QueueScheduler::default()),
        SchedulerKind::Rand => TalonScheduler::Rand(RandScheduler::new()),
    };

    #[allow(static_mut_refs)]
    let observer = unsafe { StdMapObserver::new("signals", &mut SIGNALS) };
    let mut feedback = MaxMapFeedback::new(&observer);
    let mut objective = CrashFeedback::new();

    let monitor = SimpleMonitor::new(|s| println!("{s}"));
    let mut mgr = SimpleEventManager::new(monitor);

    let mut state = StdState::new(
        StdRand::with_seed(args.seed),
        InMemoryCorpus::new(),
        OnDiskCorpus::new(&args.crashes_dir).expect("Failed to create crashes dir"),
        &mut feedback,
        &mut objective,
    )?;

    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(PowerMutationalStage::<_, EnergyScore, _, _, _, _, _>::new(
        mutator
    ));

    let mut harness = |input: &BytesInput| {
        let bytes = input.target_bytes();
        let buf = bytes.as_ref();

        unsafe {
            target_function(buf.as_ptr(), buf.len());
        }

        ExitKind::Ok
    };

    let mut executor = InProcessExecutor::builder()
        .harness(&mut harness)
        .observers(tuple_list!(observer))
        .fuzzer(&mut fuzzer)
        .state(&mut state)
        .event_mgr(&mut mgr)
        .build::<BytesInput, CrashFeedback>()?;

    if state.corpus().count() == 0 {
        let initial_seed = BytesInput::new(vec![b'A'; 16]);
        fuzzer.add_input(&mut state, &mut executor, &mut mgr, initial_seed)?;
    }

    fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;

    Ok(())
}
