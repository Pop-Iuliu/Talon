use libafl::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

pub struct DirectedScheduler {
    pub distances: HashMap<usize, f64>,
    pub temperature: f64,
}

impl DirectedScheduler {
    pub fn new(json_path: &str, initial_temp: f64) -> Self {
        let file = File::open(json_path).expect("Nu am putut deschide distances.json");
        let reader = BufReader::new(file);
        let raw_json: HashMap<String, u32> =
            serde_json::from_reader(reader).expect("Eroare la parsarea JSON-ului");

        let mut distances = HashMap::new();
        for (addr_hex, dist) in raw_json {
            let clean_hex = addr_hex.trim_start_matches("0x");
            if let Ok(addr) = usize::from_str_radix(clean_hex, 16) {
                distances.insert(addr, dist as f64);
            }
        }

        Self {
            distances,
            temperature: initial_temp,
        }
    }

    pub fn calculate_seed_distance(&self, hit_blocks: &[usize]) -> f64 {
        let mut sum_reciprocal = 0.0;
        let mut count = 0.0;

        for block in hit_blocks {
            if let Some(&dist) = self.distances.get(block) {
                let safe_dist = if dist == 0.0 { 0.1 } else { dist };
                sum_reciprocal += 1.0 / safe_dist;
                count += 1.0;
            }
        }

        if count == 0.0 {
            return 1000.0; // daca nu atinge nimic, penalizat maxim
        }

        count / sum_reciprocal
    }

    // p(s) = (1 / d(s))^(1/T)
    pub fn calculate_energy(&self, distance: f64) -> u32 {
        let inv_dist = 1.0 / distance;
        let power = 1.0 / self.temperature;

        let raw_energy = inv_dist.powf(power) * 100.0;

        (raw_energy as u32).clamp(1, 1000)
    }
}
