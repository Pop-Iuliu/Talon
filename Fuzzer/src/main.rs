use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use libafl::monitors::tui::TuiMonitor;
use libafl::{
    corpus::{Corpus, InMemoryCorpus, OnDiskCorpus, Testcase},
    events::SimpleEventManager,
    executors::inprocess::InProcessExecutor,
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    inputs::{BytesInput, HasTargetBytes},
    monitors::SimpleMonitor,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::StdMapObserver,
    schedulers::RandScheduler,
    stages::StdMutationalStage,
    state::{HasCorpus, StdState},
    Error,
};
use libafl_bolts::{rands::StdRand, tuples::tuple_list};
use serde::Deserialize;
mod scheduler;
use scheduler::DirectedDistanceScheduler;

const MAP_SIZE: usize = 65536;
static mut SIGNALS: [u8; MAP_SIZE] = [0; MAP_SIZE];

#[derive(Debug, Deserialize)]
struct RawDistances(HashMap<String, u32>);

fn main() -> Result<(), Error> {
    println!("[+] Starting Directed LibAFL Core...");

    let dist_scheduler = DirectedDistanceScheduler::new("distances.json", 5.0);
    println!(
        "[+] Loaded {} block distances from distances.json",
        dist_scheduler.distances.len()
    );

    let observer = unsafe { StdMapObserver::new("signals", &mut SIGNALS) };

    let mut feedback = MaxMapFeedback::new(&observer);
    let mut objective = CrashFeedback::new();

    let mut state = StdState::new(
        StdRand::with_seed(1337),
        InMemoryCorpus::new(),
        OnDiskCorpus::new(PathBuf::from("./crashes"))?,
        &mut feedback,
        &mut objective,
    )?;

    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    let monitor = TuiMonitor::builder()
        .title("Directed Greybox Fuzzer")
        .enhanced_graphics(false)
        .build();

    let mut mgr = SimpleEventManager::<BytesInput, _, _>::new(monitor);

    let mut fuzzer = StdFuzzer::new(dist_scheduler, feedback, objective);

    let mut harness = |input: &BytesInput| {
        let bytes = input.target_bytes();
        let buf = bytes.as_ref();

        unsafe {
            SIGNALS.fill(0);
        }

        if buf.len() >= 5 {
            unsafe {
                SIGNALS[0x1149 % MAP_SIZE] += 1;
                if buf[0] == b'T' {
                    SIGNALS[0x1162 % MAP_SIZE] += 1;
                    if buf[1] == b'A' {
                        SIGNALS[0x116c % MAP_SIZE] += 1;
                        if buf[2] == b'L' {
                            return libafl::executors::ExitKind::Crash;
                        }
                    }
                }
            }
        }
        libafl::executors::ExitKind::Ok
    };

    #[allow(deprecated)]
    let mut executor = InProcessExecutor::new(
        &mut harness,
        tuple_list!(observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
    )?;

    if state.corpus().count() == 0 {
        let initial_seed = BytesInput::new(vec![b'A', b'B', b'C']);
        state.corpus_mut().add(Testcase::new(initial_seed))?;
    }

    println!("[+] Initialized corpus. Entering fuzzing loop...");
    fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;

    Ok(())
}
