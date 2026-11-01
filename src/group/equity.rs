use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};

use crate::{
    dist::{sample, texture},
    game::{
        board::Street,
        card::{Card, CardSet},
        showdown,
    },
    group::{
        group::Group,
        index::{SuitIndex, choose},
        vectordb::{EquityVector, U, Vectordb},
    },
    ver::{Versioned, try_load},
    versions,
};

pub fn to_cdf(pdf: &[f32]) -> Vec<f32> {
    let mut cumul = 0.0;
    let mut out = vec![0.0; pdf.len()];
    for (i, x) in pdf.iter().enumerate() {
        cumul += x;
        out[i] = cumul;
    }

    out
}

pub fn to_pdf(cdf: &[f32]) -> Vec<f32> {
    let mut delta = vec![0.0; cdf.len()];
    for (i, x) in cdf.iter().enumerate() {
        let prev = if i == 0 { 0.0 } else { cdf[i - 1] };
        delta[i] = x - prev;
    }

    delta
}

const KMEANS_ITERS: usize = 200;

/// hand equity distribution k means sample indexing
fn kmeans(
    samples: usize,
    num_centroids: usize,
    bins: usize,
    street: Street,
    rng: &mut impl Rng,
    get_equity: impl Fn(CardSet, CardSet) -> Vec<f32>,
) -> Vec<EquityVector> {
    println!(
        "info equity {} estimated memory {} MB",
        street.to_string(),
        bins * size_of::<f32>() * num_centroids / 1024 / 1024
    );

    // sample points
    let mut points: Vec<EquityVector> = vec![];
    points.reserve_exact(samples);
    for i in 0..samples {
        if i % 100 == 0 {
            println!("debug samples {}", i);
        }

        let (hand, community) = CardSet::sample2(rng, 2, street.visible_community());
        let equity = get_equity(hand, community);
        points.push(equity);
    }

    let mut centroids: Vec<EquityVector> = rand::seq::index::sample(rng, samples, num_centroids)
        .into_iter()
        .map(|i| points[i].clone())
        .collect();

    let mut db = U::new(bins);

    // k means iteration
    for iter in 0..KMEANS_ITERS {
        if iter % 10 == 0 {
            println!("debug kmeans iter {}", iter);
        }

        db.reset();
        db.reserve(num_centroids);
        for (i, centroid) in centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        // find closest centroid per point
        let mut new_centroids: Vec<Vec<usize>> = vec![vec![]; num_centroids];
        for (i, point) in points.iter().enumerate() {
            let (centroid, _distance) = db.query(&point);
            new_centroids[centroid as usize].push(i);
        }

        // each centroid = average of points
        for (centroid, close_points) in new_centroids.iter().enumerate() {
            if close_points.is_empty() {
                // this can be bad
                continue;
            }

            let centroid = &mut centroids[centroid];
            centroid.fill(0.0);
            for p in close_points {
                for k in 0..bins {
                    centroid[k] += points[*p][k] / close_points.len() as f32;
                }
            }
        }

        if iter % 10 == 0 {
            // within-cluster-sum-of-squares
            let mut cost = 0.0;
            for sample in points.iter() {
                let (_centroid, distance) = db.query(&sample);
                cost += distance * distance;
            }

            println!(
                "info within_cluster_sum_of_squares {}",
                cost / centroids.len() as f32
            );
        }
    }

    centroids
}

#[derive(Serialize, Deserialize)]
struct EquityData {
    centroids: Vec<EquityVector>,
}

pub struct PreflopGroup {
    db: Box<dyn Vectordb>,
    centroids: Vec<EquityVector>,
    equity: sample::Preflop,
    index: SuitIndex,
}

versions!(PreflopGroupVersions, V1);

impl Versioned<PreflopGroupVersions> for PreflopGroup {
    fn build(version: &PreflopGroupVersions, path: &str) -> Self {
        match version {
            PreflopGroupVersions::V1 => {
                let rng = &mut rand::rng();
                let equity: sample::Preflop = try_load(&sample::SamplePreflopVersions::V1, path);
                let index = SuitIndex::new(2, 0);
                let centroids = kmeans(
                    10000,
                    20,
                    sample::BINS,
                    Street::Preflop,
                    rng,
                    |hand, _community| equity.equity(hand, &index).to_vec(),
                );

                let mut db = Box::new(U::new(sample::BINS));
                db.reserve(20);
                for (i, centroid) in centroids.iter().enumerate() {
                    db.store(i as u64, centroid);
                }

                Self {
                    db,
                    centroids,
                    equity,
                    index,
                }
            }
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let data = EquityData {
            centroids: self.centroids.clone(),
        };
        postcard::to_allocvec(&data).unwrap()
    }

    fn from_bytes(_version: &PreflopGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();

        let mut db = Box::new(U::new(sample::BINS));
        db.reserve(20);
        for (i, centroid) in data.centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        let equity: sample::Preflop = try_load(&sample::SamplePreflopVersions::V1, path);
        let index = SuitIndex::new(2, 0);

        Self {
            db,
            centroids: data.centroids,
            equity,
            index,
        }
    }
}

impl Group for PreflopGroup {
    fn map(&self, hand: CardSet, _community: CardSet) -> usize {
        let equity = self.equity.equity(hand, &self.index).to_vec();
        let (i, _distance) = self.db.query(&equity);
        i as usize
    }

    fn size(&self) -> usize {
        self.centroids.len()
    }

    fn repr(&self, i: usize) -> Vec<f32> {
        self.centroids[i].clone()
    }
}

pub struct FlopGroup {
    db: Box<dyn Vectordb>,
    centroids: Vec<EquityVector>,
    equity: texture::Flop,
    index: SuitIndex,
}

versions!(FlopGroupVersions, V1);

impl Versioned<FlopGroupVersions> for FlopGroup {
    fn build(version: &FlopGroupVersions, path: &str) -> Self {
        match version {
            FlopGroupVersions::V1 => {
                let rng = &mut rand::rng();
                let equity: texture::Flop = try_load(&texture::TextureFlopVersions::V1, path);
                let index = SuitIndex::new(2, 3);
                let centroids = kmeans(
                    10000,
                    20,
                    texture::BINS,
                    Street::Flop,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                let mut db = Box::new(U::new(texture::BINS));
                db.reserve(20);
                for (i, centroid) in centroids.iter().enumerate() {
                    db.store(i as u64, centroid);
                }

                Self {
                    db,
                    centroids,
                    equity,
                    index,
                }
            }
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let data = EquityData {
            centroids: self.centroids.clone(),
        };
        postcard::to_allocvec(&data).unwrap()
    }

    fn from_bytes(_version: &FlopGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();

        let mut db = Box::new(U::new(texture::BINS));
        db.reserve(20);
        for (i, centroid) in data.centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        let equity: texture::Flop = try_load(&texture::TextureFlopVersions::V1, path);
        let index = SuitIndex::new(2, 3);

        Self {
            db,
            centroids: data.centroids,
            equity,
            index,
        }
    }
}

impl Group for FlopGroup {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        let equity = self.equity.equity(hand, community, &self.index).to_vec();
        let (i, _distance) = self.db.query(&equity);
        i as usize
    }

    fn size(&self) -> usize {
        self.centroids.len()
    }

    fn repr(&self, i: usize) -> Vec<f32> {
        self.centroids[i].clone()
    }
}

pub struct TurnGroup {
    db: Box<dyn Vectordb>,
    centroids: Vec<EquityVector>,
    equity: texture::Turn,
    index: SuitIndex,
}

versions!(TurnGroupVersions, V1);

impl Versioned<TurnGroupVersions> for TurnGroup {
    fn build(version: &TurnGroupVersions, path: &str) -> Self {
        match version {
            TurnGroupVersions::V1 => {
                let rng = &mut rand::rng();
                let equity: texture::Turn = try_load(&texture::TextureTurnVersions::V1, path);
                let index = SuitIndex::new(2, 4);
                let centroids = kmeans(
                    10000,
                    8,
                    texture::BINS,
                    Street::Turn,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                let mut db = Box::new(U::new(texture::BINS));
                db.reserve(8);
                for (i, centroid) in centroids.iter().enumerate() {
                    db.store(i as u64, centroid);
                }

                Self {
                    db,
                    centroids,
                    equity,
                    index,
                }
            }
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let data = EquityData {
            centroids: self.centroids.clone(),
        };
        postcard::to_allocvec(&data).unwrap()
    }

    fn from_bytes(_version: &TurnGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();

        let mut db = Box::new(U::new(texture::BINS));
        db.reserve(8);
        for (i, centroid) in data.centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        let equity: texture::Turn = try_load(&texture::TextureTurnVersions::V1, path);
        let index = SuitIndex::new(2, 4);

        Self {
            db,
            centroids: data.centroids,
            equity,
            index,
        }
    }
}

impl Group for TurnGroup {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        let equity = self.equity.equity(hand, community, &self.index).to_vec();
        let (i, _distance) = self.db.query(&equity);
        i as usize
    }

    fn size(&self) -> usize {
        self.centroids.len()
    }

    fn repr(&self, i: usize) -> Vec<f32> {
        self.centroids[i].clone()
    }
}

// TODO: consider better textures (distribution like)
pub struct RiverGroup {
    db: Box<dyn Vectordb>,
    centroids: Vec<EquityVector>,
    equity: texture::River,
    index: SuitIndex,
}

versions!(RiverGroupVersions, V1);

impl Versioned<RiverGroupVersions> for RiverGroup {
    fn build(version: &RiverGroupVersions, path: &str) -> Self {
        match version {
            RiverGroupVersions::V1 => {
                let rng = &mut rand::rng();
                let equity: texture::River = try_load(&texture::TextureRiverVersions::V1, path);
                let index = SuitIndex::new(2, 5);
                let centroids = kmeans(10000, 5, 1, Street::Turn, rng, |hand, community| {
                    vec![equity.equity(hand, community, &index)]
                });

                let mut db = Box::new(U::new(1));
                db.reserve(5);
                for (i, centroid) in centroids.iter().enumerate() {
                    db.store(i as u64, centroid);
                }

                Self {
                    db,
                    centroids,
                    equity,
                    index,
                }
            }
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let data = EquityData {
            centroids: self.centroids.clone(),
        };
        postcard::to_allocvec(&data).unwrap()
    }

    fn from_bytes(_version: &RiverGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();

        let mut db = Box::new(U::new(1));
        db.reserve(5);
        for (i, centroid) in data.centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        let equity: texture::River = try_load(&texture::TextureRiverVersions::V1, path);
        let index = SuitIndex::new(2, 5);

        Self {
            db,
            centroids: data.centroids,
            equity,
            index,
        }
    }
}

impl Group for RiverGroup {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        let equity = vec![self.equity.equity(hand, community, &self.index)];
        let (i, _distance) = self.db.query(&equity);
        i as usize
    }

    fn size(&self) -> usize {
        self.centroids.len()
    }

    fn repr(&self, i: usize) -> Vec<f32> {
        self.centroids[i].clone()
    }
}
