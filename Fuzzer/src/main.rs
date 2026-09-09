use std::collections::HashMap;
use std::path::PathBuf;

use libafl::{
    corpus::{Corpus, InMemoryCorpus, OnDiskCorpus},
    events::SimpleEventManager,
    executors::{inprocess::InProcessExecutor, ExitKind},
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Evaluator, Fuzzer, StdFuzzer},
    inputs::{BytesInput, HasTargetBytes},
    monitors::SimpleMonitor,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::StdMapObserver,
    stages::StdMutationalStage,
    state::{HasCorpus, StdState},
    Error,
};
use libafl_bolts::{current_nanos, rands::StdRand, tuples::tuple_list};
use serde::Deserialize;

mod scheduler;
use scheduler::DirectedDistanceScheduler;

extern "C" {
    fn target_function(data: *const u8, size: usize);
}

const MAP_SIZE: usize = 65536;
#[no_mangle]
static mut SIGNALS: [u8; MAP_SIZE] = [0; MAP_SIZE];

#[derive(Debug, Deserialize)]
struct RawDistances(HashMap<String, u32>);

pub fn main() -> Result<(), Error> {
    // 1. Scheduler
    let dist_scheduler = DirectedDistanceScheduler::new("distances.json", 5.0);

    #[allow(static_mut_refs)]
    let observer = unsafe { StdMapObserver::new("signals", &mut SIGNALS) };
    let mut feedback = MaxMapFeedback::new(&observer);
    let mut objective = CrashFeedback::new();

    let monitor = SimpleMonitor::new(|s| println!("{s}"));
    let mut mgr = SimpleEventManager::new(monitor);

    let mut state = StdState::new(
        StdRand::with_seed(current_nanos()),
        InMemoryCorpus::new(),
        OnDiskCorpus::new(PathBuf::from("./crashes")).expect("Failed to create crashes dir"),
        &mut feedback,
        &mut objective,
    )?;

    let mut fuzzer = StdFuzzer::new(dist_scheduler, feedback, objective);
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    let mut harness = |input: &BytesInput| {
        let bytes = input.target_bytes();
        let buf = bytes.as_ref();

        unsafe {
            std::ptr::write_bytes(std::ptr::addr_of_mut!(SIGNALS) as *mut u8, 0, MAP_SIZE);
            target_function(buf.as_ptr(), buf.len());
        }

        ExitKind::Ok
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
        fuzzer.add_input(&mut state, &mut executor, &mut mgr, initial_seed)?;
    }

    fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;

    Ok(())
}
