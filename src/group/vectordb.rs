use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

pub type EquityVector = Vec<f32>;

pub trait Vectordb {
    fn store(&mut self, key: u64, x: &EquityVector);
    fn query(&self, x: &EquityVector) -> (u64, f32);
    fn reset(&mut self);
}

pub struct U {
    index: Index,
    dim: usize,
}

impl U {
    pub fn new(dim: usize) -> Self {
        let mut options = IndexOptions::default();
        options.dimensions = dim;
        options.metric = MetricKind::Unknown;
        options.quantization = ScalarKind::F32;

        let mut index = Index::new(&options).expect("Failed to create index.");

        // Use L1 distance for emd
        let d = dim;
        index.change_metric::<f32>(Box::new(move |a, b| {
            let (a, b) = unsafe {
                (
                    std::slice::from_raw_parts(a, d),
                    std::slice::from_raw_parts(b, d),
                )
            };
            a.iter().zip(b).map(|(a, b)| (a - b).abs()).sum()
        }));
        Self { index, dim }
    }
}

impl Vectordb for U {
    fn store(&mut self, key: u64, x: &EquityVector) {
        self.index.add(key as u64, x).expect("Failed to store");
    }

    fn query(&self, x: &EquityVector) -> (u64, f32) {
        let results = self.index.search(&x, 1).expect("Failed to query");
        let (key, distance) = results
            .keys
            .iter()
            .zip(results.distances.iter())
            .next()
            .expect("Expected one result");
        (*key, *distance)
    }

    fn reset(&mut self) {
        self.index.reset().expect("Failed to reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_uses_l1_instead_of_squared_l2() {
        let mut db = U::new(4);
        db.index.reserve(2).unwrap();
        db.store(1, &vec![3.0, 0.0, 0.0, 0.0]);
        db.store(2, &vec![1.0, 1.0, 1.0, 1.0]);

        // Squared L2 would prefer key 2 (4 < 9); L1 prefers key 1 (3 < 4).
        assert_eq!(db.query(&vec![0.0; 4]), (1, 3.0));
    }
}
