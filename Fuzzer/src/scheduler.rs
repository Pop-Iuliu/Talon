use libafl::common::HasMetadata;
use libafl::{
    corpus::{Corpus, CorpusId, Testcase},
    inputs::BytesInput,
    random_corpus_id,
    schedulers::{Scheduler, TestcaseScore},
    state::{HasCorpus, HasRand},
    Error,
};
use libafl_bolts::{rands::Rand, tuples::MatchName, SerdeAny};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path, sync::Arc};

pub const MAP_SIZE: usize = 4096;
const SCHEMA_VERSION: u32 = 1;
const COOLING_FLOOR: f64 = 0.01;
const WEIGHT_FLOOR: f64 = 0.01;
const BASE_ENERGY: usize = 128;

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

/// Raw hop-count distance of a corpus entry, recorded when it was added.
#[derive(Debug, Serialize, Deserialize, SerdeAny)]
pub struct SeedDistanceMetadata {
    pub distance: f64,
}

/// Annealing progress shared with the power stage through state metadata.
#[derive(Debug, Serialize, Deserialize, SerdeAny)]
pub struct AnnealMetadata {
    pub temperature: f64,
    pub min_dist: f64,
    pub max_dist: f64,
}

impl Default for AnnealMetadata {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            min_dist: 0.0,
            max_dist: 0.0,
        }
    }
}

/// Scale a raw hop count into [0, 1] with the running min and max over the
/// corpus, the way AFLGo does. A degenerate range maps to 0.0.
pub fn normalize_distance(raw: f64, min: f64, max: f64) -> f64 {
    if max > min {
        ((raw - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// AFLGo-style energy: near seeds get more mutations the colder it is, and
/// the temperature equalizes the energy when the campaign is hot.
pub fn calculate_energy(temperature: f64, norm_distance: f64, base_energy: usize) -> usize {
    let factor = ((1.0 - norm_distance) * (1.0 - temperature) + 0.5 * temperature).powi(2);
    ((base_energy as f64) * factor).max(1.0) as usize
}

/// Mean hop-count distance over the blocks a seed hit, plus how many of the
/// known blocks it matched. None when the seed matched no known block.
fn signals_distance(distances: &DistanceMap, signals: &[u8]) -> Option<(usize, f64)> {
    let mut matched = 0;
    let mut total_dist = 0.0;
    for (addr, &hit) in signals.iter().enumerate() {
        if hit > 0 {
            if let Some(&dist) = distances.get(&addr.to_string()) {
                matched += 1;
                total_dist += dist as f64;
            }
        }
    }
    (matched > 0).then_some((matched, total_dist / matched as f64))
}

pub struct DirectedDistanceScheduler {
    pub distances: Arc<DistanceMap>,
    clock: Box<dyn Clock + Send>,
    cooling_time_secs: f64,
    last_distance: Option<(usize, f64)>,
}

impl DirectedDistanceScheduler {
    pub fn new<P: AsRef<Path>>(path: P, cooling_time_secs: f64) -> Result<Self, Error> {
        let path = path.as_ref();
        let json = fs::read_to_string(path).map_err(|e| {
            Error::os_error(e, format!("failed to read distance map {}", path.display()))
        })?;
        let (distances, target_id) = parse_distance_file(&json)?;
        println!(
            "directed scheduler: loaded {} distances, target id {}",
            distances.len(),
            target_id.map_or_else(|| "none".to_string(), |id| id.to_string())
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
            clock,
            cooling_time_secs,
            last_distance: None,
        }
    }

    fn current_temperature(&self) -> f64 {
        let elapsed = self.clock.elapsed_secs();
        if elapsed >= self.cooling_time_secs {
            COOLING_FLOOR
        } else {
            1.0 - (elapsed / self.cooling_time_secs)
        }
    }

    fn pick_weighted(
        &self,
        state: &mut (impl HasCorpus<BytesInput> + HasMetadata + HasRand),
    ) -> Result<CorpusId, Error> {
        let (min_dist, max_dist) = {
            let anneal = state.metadata_or_insert_with(AnnealMetadata::default);
            (anneal.min_dist, anneal.max_dist)
        };

        let mut cumulative = Vec::new();
        let mut total_weight = 0.0;
        for id in state.corpus().ids() {
            let raw = state
                .corpus()
                .get(id)?
                .borrow()
                .metadata_map()
                .get::<SeedDistanceMetadata>()
                .map(|m| m.distance)
                .unwrap_or(1.0);
            let norm = normalize_distance(raw, min_dist, max_dist);
            total_weight += 1.0 - norm + WEIGHT_FLOOR;
            cumulative.push((id, total_weight));
        }

        let threshold = state.rand_mut().next_float() * total_weight;
        cumulative
            .iter()
            .find(|(_, acc)| threshold < *acc)
            .or(cumulative.last())
            .map(|(id, _)| *id)
            .ok_or_else(|| Error::empty("corpus is empty"))
    }
}

impl<S> Scheduler<BytesInput, S> for DirectedDistanceScheduler
where
    S: HasCorpus<BytesInput> + HasMetadata + HasRand,
{
    fn on_add(&mut self, state: &mut S, id: CorpusId) -> Result<(), Error> {
        let current_id = *state.corpus().current();
        state
            .corpus()
            .get(id)?
            .borrow_mut()
            .set_parent_id_optional(current_id);

        if let Some((matched, distance)) = self.last_distance.take() {
            println!("seed {id}: {matched} matched blocks, distance {distance:.2}");

            state
                .corpus()
                .get(id)?
                .borrow_mut()
                .add_metadata(SeedDistanceMetadata { distance });

            let anneal = state.metadata_or_insert_with(AnnealMetadata::default);
            anneal.min_dist = anneal.min_dist.min(distance);
            anneal.max_dist = anneal.max_dist.max(distance);
        }
        Ok(())
    }

    fn on_evaluation<OT>(
        &mut self,
        _state: &mut S,
        _input: &BytesInput,
        _observers: &OT,
    ) -> Result<(), Error>
    where
        OT: MatchName,
    {
        #[allow(static_mut_refs)]
        let signals: &[u8] = unsafe { &crate::SIGNALS };
        self.last_distance = signals_distance(&self.distances, signals);
        Ok(())
    }

    fn next(&mut self, state: &mut S) -> Result<CorpusId, Error> {
        if state.corpus().count() == 0 {
            return Err(Error::empty("corpus is empty"));
        }

        let temperature = self.current_temperature();
        {
            let anneal = state.metadata_or_insert_with(AnnealMetadata::default);
            anneal.temperature = temperature;
        }

        let next_id = if state.rand_mut().coinflip(temperature) {
            random_corpus_id!(state.corpus(), state.rand_mut())
        } else {
            self.pick_weighted(state)?
        };

        <Self as Scheduler<BytesInput, S>>::set_current_scheduled(self, state, Some(next_id))?;
        Ok(next_id)
    }

    fn set_current_scheduled(
        &mut self,
        state: &mut S,
        next_id: Option<CorpusId>,
    ) -> Result<(), Error> {
        *state.corpus_mut().current_mut() = next_id;
        Ok(())
    }
}

/// FIFO round-robin queue over the corpus, as a baseline.
#[derive(Default)]
pub struct QueueScheduler {
    next_index: usize,
}

impl<I, S> Scheduler<I, S> for QueueScheduler
where
    S: HasCorpus<I>,
{
    fn on_add(&mut self, state: &mut S, id: CorpusId) -> Result<(), Error> {
        let current_id = *state.corpus().current();
        state
            .corpus()
            .get(id)?
            .borrow_mut()
            .set_parent_id_optional(current_id);
        Ok(())
    }

    fn next(&mut self, state: &mut S) -> Result<CorpusId, Error> {
        if state.corpus().count() == 0 {
            return Err(Error::empty("corpus is empty"));
        }
        let id = state.corpus().nth(self.next_index % state.corpus().count());
        self.next_index = self.next_index.wrapping_add(1);
        <Self as Scheduler<I, S>>::set_current_scheduled(self, state, Some(id))?;
        Ok(id)
    }

    fn set_current_scheduled(
        &mut self,
        state: &mut S,
        next_id: Option<CorpusId>,
    ) -> Result<(), Error> {
        *state.corpus_mut().current_mut() = next_id;
        Ok(())
    }
}

/// Energy through the power schedule: near and cold seeds get more
/// mutations. Without annealing metadata (the baselines) every seed gets a
/// fixed budget.
pub struct EnergyScore;

impl<S> TestcaseScore<BytesInput, S> for EnergyScore
where
    S: HasCorpus<BytesInput> + HasMetadata,
{
    fn compute(state: &S, entry: &mut Testcase<BytesInput>) -> Result<f64, Error> {
        let energy = match state.metadata_map().get::<AnnealMetadata>() {
            Some(anneal) => {
                let raw = entry
                    .metadata_map()
                    .get::<SeedDistanceMetadata>()
                    .map(|m| m.distance)
                    .unwrap_or(1.0);
                let norm = normalize_distance(raw, anneal.min_dist, anneal.max_dist);
                calculate_energy(anneal.temperature, norm, BASE_ENERGY)
            }
            None => BASE_ENERGY,
        };
        Ok(energy as f64)
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
        assert_eq!(s.current_temperature(), COOLING_FLOOR);

        micros.store(50_000_000, Ordering::SeqCst);
        assert_eq!(s.current_temperature(), COOLING_FLOOR);
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
    fn signal_distance_is_the_mean_over_hit_blocks() {
        let distances = HashMap::from([("100".to_string(), 2), ("102".to_string(), 4)]);

        let mut signals = vec![0u8; 103];
        signals[100] = 1;
        signals[102] = 1;
        assert_eq!(signals_distance(&distances, &signals), Some((2, 3.0)));
    }

    #[test]
    fn signal_distance_is_none_without_hits() {
        let distances = HashMap::from([("100".to_string(), 2)]);
        assert_eq!(signals_distance(&distances, &vec![0u8; 103]), None);
    }

    #[test]
    fn loader_accepts_the_current_schema() {
        let json = r#"{"version":1,"target_id":103,"map_size":4096,"distances":{"100":6,"103":0}}"#;
        let (distances, target_id) = parse_distance_file(json).unwrap();
        assert_eq!(distances.len(), 2);
        assert_eq!(target_id, Some(103));
    }

    #[test]
    fn loader_rejects_a_wrong_version() {
        let json = r#"{"version":2,"target_id":103,"map_size":4096,"distances":{}}"#;
        let err = parse_distance_file(json).unwrap_err();
        assert!(err.to_string().contains("version 2"));
    }

    #[test]
    fn loader_rejects_keys_beyond_the_map_size() {
        let json = r#"{"version":1,"target_id":null,"map_size":4096,"distances":{"65536":1}}"#;
        let err = parse_distance_file(json).unwrap_err();
        assert!(err.to_string().contains("map size"));
    }

    #[test]
    fn loader_rejects_a_map_size_mismatch() {
        let json = r#"{"version":1,"target_id":null,"map_size":1024,"distances":{}}"#;
        let err = parse_distance_file(json).unwrap_err();
        assert!(err.to_string().contains("map_size is 1024"));
    }

    #[test]
    fn normalization_maps_the_range_into_the_unit_interval() {
        assert_eq!(normalize_distance(5.0, 0.0, 10.0), 0.5);
        assert_eq!(normalize_distance(5.0, 5.0, 5.0), 0.0);
        assert_eq!(normalize_distance(3.0, 0.0, 2.0), 1.0);
        assert_eq!(normalize_distance(-1.0, 0.0, 2.0), 0.0);
    }

    #[test]
    fn near_seeds_get_more_energy_when_cold() {
        let near = calculate_energy(0.1, 0.0, BASE_ENERGY);
        let far = calculate_energy(0.1, 1.0, BASE_ENERGY);
        assert!(near > far);
        assert!(near > BASE_ENERGY / 2);
        assert_eq!(far, 1);
    }

    #[test]
    fn temperature_equalizes_energy_when_hot() {
        let hot = calculate_energy(1.0, 0.0, BASE_ENERGY);
        let also_hot = calculate_energy(1.0, 1.0, BASE_ENERGY);
        assert_eq!(hot, also_hot);
    }
}
