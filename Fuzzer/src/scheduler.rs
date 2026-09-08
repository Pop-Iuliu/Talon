use libafl::{
    corpus::{Corpus, CorpusId},
    inputs::BytesInput,
    schedulers::Scheduler,
    state::HasCorpus,
    Error,
};
use std::fs;
use std::{collections::HashMap, fs::File, io::BufReader, path::Path, sync::Arc};
type DistanceMap = HashMap<String, u32>;
pub struct DirectedDistanceScheduler {
    pub distances: Arc<DistanceMap>,
    pub seed_distances: HashMap<CorpusId, f64>,
    start_time: std::time::Instant,
    cooling_time_secs: f64,
}

impl DirectedDistanceScheduler {
    pub fn new<P: AsRef<Path>>(path: P, cooling_time_secs: f64) -> Self {
        let file = File::open(path)
            .unwrap_or_else(|_| panic!("Nu s-a putut deschide fisierul de distante!"));
        let reader = BufReader::new(file);
        let json_content = fs::read_to_string("distances.json").expect("Failed to read JSON file");
        let distances: DistanceMap = match serde_json::from_str(&json_content) {
            Ok(data) => data,
            Err(e) => panic!("Eroare la parsarea JSON-ului de distante! Detalii: {}", e),
        };

        Self {
            distances: Arc::new(distances),
            seed_distances: HashMap::new(),
            start_time: std::time::Instant::now(),
            cooling_time_secs,
        }
    }

    fn current_temperature(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed >= self.cooling_time_secs {
            0.01
        } else {
            1.0 - (elapsed / self.cooling_time_secs)
        }
    }

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
    fn on_add(&mut self, _state: &mut S, _idx: CorpusId) -> Result<(), Error> {
        Ok(())
    }

    fn next(&mut self, state: &mut S) -> Result<CorpusId, Error> {
        let corpus = state.corpus();
        if corpus.count() == 0 {
            return Err(Error::empty("Corpus-ul este gol!"));
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
