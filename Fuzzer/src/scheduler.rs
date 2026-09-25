use libafl::{
    corpus::{Corpus, CorpusId},
    inputs::BytesInput,
    schedulers::Scheduler,
    state::HasCorpus,
    Error,
};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path, sync::Arc};

pub const MAP_SIZE: usize = 65536;
const SCHEMA_VERSION: u32 = 1;

type DistanceMap = HashMap<String, u32>;

trait Clock {
    fn elapsed_secs(&self) -> f64;
}

struct RealClock {
    start: std::time::Instant,
}

impl Clock for RealClock {
    fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

#[derive(Debug, Deserialize)]
struct DistanceFile {
    version: u32,
    target_id: Option<u32>,
    map_size: usize,
    distances: HashMap<String, u32>,
}

fn parse_distance_file(json: &str) -> Result<(DistanceMap, Option<u32>), Error> {
    let file: DistanceFile = serde_json::from_str(json)
        .map_err(|e| Error::unknown(format!("failed to parse distance map: {e}")))?;
    if file.version != SCHEMA_VERSION {
        return Err(Error::unknown(format!(
            "unsupported distance map version {} (expected {SCHEMA_VERSION})",
            file.version
        )));
    }
    if file.map_size != MAP_SIZE {
        return Err(Error::unknown(format!(
            "distance map map_size is {} but the fuzzer map size is {MAP_SIZE}",
            file.map_size
        )));
    }

    let mut distances = HashMap::new();
    for (key, dist) in file.distances {
        let id: usize = key
            .parse()
            .map_err(|_| Error::unknown(format!("non-numeric distance key '{key}'")))?;
        if id >= MAP_SIZE {
            return Err(Error::unknown(format!(
                "distance key {id} is not below the map size {MAP_SIZE}"
            )));
        }
        distances.insert(id.to_string(), dist);
    }
    Ok((distances, file.target_id))
}

pub struct DirectedDistanceScheduler {
    pub distances: Arc<DistanceMap>,
    pub seed_distances: HashMap<CorpusId, f64>,
    clock: Box<dyn Clock + Send>,
    cooling_time_secs: f64,
}

impl DirectedDistanceScheduler {
    pub fn new<P: AsRef<Path>>(path: P, cooling_time_secs: f64) -> Result<Self, Error> {
        let path = path.as_ref();
        let json = fs::read_to_string(path).map_err(|e| {
            Error::os_error(e, format!("failed to read distance map {}", path.display()))
        })?;
        let (distances, target_id) = parse_distance_file(&json)?;
        println!(
            "directed scheduler: loaded {} distances, target id {:?}",
            distances.len(),
            target_id
        );

        Ok(Self::with_clock(
            distances,
            cooling_time_secs,
            Box::new(RealClock {
                start: std::time::Instant::now(),
            }),
        ))
    }

    fn with_clock(
        distances: DistanceMap,
        cooling_time_secs: f64,
        clock: Box<dyn Clock + Send>,
    ) -> Self {
        Self {
            distances: Arc::new(distances),
            seed_distances: HashMap::new(),
            clock,
            cooling_time_secs,
        }
    }

    fn current_temperature(&self) -> f64 {
        let elapsed = self.clock.elapsed_secs();
        if elapsed >= self.cooling_time_secs {
            0.01
        } else {
            1.0 - (elapsed / self.cooling_time_secs)
        }
    }

    // Wired into the scheduler in Sprint 3 (VI-1).
    #[allow(dead_code)]
    pub fn calculate_seed_distance(&self, signals: &[u8]) -> f64 {
        let mut total_dist = 0.0;
        let mut count = 0;

        for (addr, &hit) in signals.iter().enumerate() {
            if hit > 0 {
                if let Some(&d) = self.distances.get(&addr.to_string()) {
                    total_dist += d as f64;
                    count += 1;
                }
            }
        }

        if count == 0 {
            1.0
        } else {
            total_dist / count as f64
        }
    }

    // Wired into scoring in Sprint 3 (VI-3).
    #[allow(dead_code)]
    pub fn calculate_energy(&self, norm_distance: f64, base_energy: usize) -> usize {
        let t = self.current_temperature();
        let factor = ((1.0 - norm_distance) * (1.0 - t) + 0.5 * t).powi(2);
        ((base_energy as f64) * factor).max(1.0) as usize
    }
}

impl<S> Scheduler<BytesInput, S> for DirectedDistanceScheduler
where
    S: HasCorpus<BytesInput>,
{
    fn on_add(&mut self, _state: &mut S, idx: CorpusId) -> Result<(), Error> {
        #[allow(static_mut_refs)]
        let signals: &[u8] = unsafe { &crate::SIGNALS };

        let mut matched = 0;
        let mut total_dist = 0.0;
        for (addr, &hit) in signals.iter().enumerate() {
            if hit > 0 {
                if let Some(&dist) = self.distances.get(&addr.to_string()) {
                    matched += 1;
                    total_dist += dist as f64;
                }
            }
        }
        if matched > 0 {
            println!(
                "seed {idx:?}: {matched} matched blocks, mean distance {:.2}",
                total_dist / matched as f64
            );
        }
        Ok(())
    }

    fn next(&mut self, state: &mut S) -> Result<CorpusId, Error> {
        let corpus = state.corpus();
        if corpus.count() == 0 {
            return Err(Error::empty("corpus is empty"));
        }

        let fallback_id = corpus.last().unwrap_or_else(|| corpus.first().unwrap());
        let t = self.current_temperature();

        if t > 0.8 || self.seed_distances.is_empty() {
            return Ok(fallback_id);
        }

        let mut best_id = fallback_id;
        let mut min_dist = f64::MAX;

        for (&id, &dist) in self.seed_distances.iter() {
            if dist < min_dist {
                min_dist = dist;
                best_id = id;
            }
        }

        Ok(best_id)
    }

    fn set_current_scheduled(
        &mut self,
        _state: &mut S,
        _next_idx: Option<CorpusId>,
    ) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    struct FixedClock {
        micros: Arc<AtomicU64>,
    }

    impl Clock for FixedClock {
        fn elapsed_secs(&self) -> f64 {
            self.micros.load(Ordering::SeqCst) as f64 / 1_000_000.0
        }
    }

    fn fixed_clock() -> (Arc<AtomicU64>, Box<dyn Clock + Send>) {
        let micros = Arc::new(AtomicU64::new(0));
        (micros.clone(), Box::new(FixedClock { micros }))
    }

    #[test]
    fn temperature_starts_hot() {
        let (micros, clock) = fixed_clock();
        let s = DirectedDistanceScheduler::with_clock(HashMap::new(), 5.0, clock);
        micros.store(0, Ordering::SeqCst);
        assert_eq!(s.current_temperature(), 1.0);
    }

    #[test]
    fn temperature_cools_linearly_until_the_floor() {
        let (micros, clock) = fixed_clock();
        let s = DirectedDistanceScheduler::with_clock(HashMap::new(), 5.0, clock);

        micros.store(2_500_000, Ordering::SeqCst);
        assert_eq!(s.current_temperature(), 0.5);

        micros.store(5_000_000, Ordering::SeqCst);
        assert_eq!(s.current_temperature(), 0.01);

        micros.store(50_000_000, Ordering::SeqCst);
        assert_eq!(s.current_temperature(), 0.01);
    }

    #[test]
    fn temperature_strictly_decreases_while_cooling() {
        let (micros, clock) = fixed_clock();
        let s = DirectedDistanceScheduler::with_clock(HashMap::new(), 1.0, clock);

        let mut last = s.current_temperature();
        for step in [200_000, 400_000, 600_000, 800_000] {
            micros.store(step, Ordering::SeqCst);
            let t = s.current_temperature();
            assert!(t < last);
            last = t;
        }
    }

    #[test]
    fn seed_distance_is_the_mean_over_hit_blocks() {
        let (_, clock) = fixed_clock();
        let s = DirectedDistanceScheduler::with_clock(
            HashMap::from([("100".to_string(), 2), ("102".to_string(), 4)]),
            5.0,
            clock,
        );

        let mut signals = vec![0u8; 103];
        signals[100] = 1;
        signals[102] = 1;
        assert_eq!(s.calculate_seed_distance(&signals), 3.0);
    }

    #[test]
    fn seed_distance_falls_back_without_hits() {
        let (_, clock) = fixed_clock();
        let s = DirectedDistanceScheduler::with_clock(
            HashMap::from([("100".to_string(), 2)]),
            5.0,
            clock,
        );

        let signals = vec![0u8; 103];
        assert_eq!(s.calculate_seed_distance(&signals), 1.0);
    }

    #[test]
    fn loader_accepts_the_current_schema() {
        let json =
            r#"{"version":1,"target_id":103,"map_size":65536,"distances":{"100":6,"103":0}}"#;
        let (distances, target_id) = parse_distance_file(json).unwrap();
        assert_eq!(distances.len(), 2);
        assert_eq!(target_id, Some(103));
    }

    #[test]
    fn loader_rejects_a_wrong_version() {
        let json = r#"{"version":2,"target_id":103,"map_size":65536,"distances":{}}"#;
        let err = parse_distance_file(json).unwrap_err();
        assert!(err.to_string().contains("version 2"));
    }

    #[test]
    fn loader_rejects_keys_beyond_the_map_size() {
        let json = r#"{"version":1,"target_id":null,"map_size":65536,"distances":{"65536":1}}"#;
        let err = parse_distance_file(json).unwrap_err();
        assert!(err.to_string().contains("map size"));
    }

    #[test]
    fn loader_rejects_a_map_size_mismatch() {
        let json = r#"{"version":1,"target_id":null,"map_size":1024,"distances":{}}"#;
        let err = parse_distance_file(json).unwrap_err();
        assert!(err.to_string().contains("map_size is 1024"));
    }
}
