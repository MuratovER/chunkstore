use serde::{Deserialize, Serialize};

/// Dedup statistics tracked by the store.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Stats {
    pub total_bytes: u64,
    pub stored_bytes: u64,
}

impl Stats {
    pub fn add_reference(&mut self, size: u64) {
        self.total_bytes += size;
    }

    pub fn add_unique_chunk(&mut self, size: u64) {
        self.total_bytes += size;
        self.stored_bytes += size;
    }

    pub fn remove_reference(&mut self, size: u64) {
        self.total_bytes = self.total_bytes.saturating_sub(size);
    }

    pub fn remove_unique_chunk(&mut self, size: u64) {
        self.stored_bytes = self.stored_bytes.saturating_sub(size);
    }

    pub fn savings_pct(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        let saved = self.total_bytes.saturating_sub(self.stored_bytes) as f64;
        (saved / self.total_bytes as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn savings_for_half_dedup() {
        let mut s = Stats::default();
        s.add_unique_chunk(100);
        s.add_reference(100);
        assert!((s.savings_pct() - 50.0).abs() < f64::EPSILON);
    }
}
