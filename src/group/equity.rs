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

const KMEANS_ITERS: usize = 100;

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
        "info kmeans equity {} estimated memory {} MB",
        street.to_string(),
        bins * size_of::<f32>() * num_centroids / 1024 / 1024
    );

    // sample points
    let mut points: Vec<EquityVector> = vec![];
    points.reserve_exact(samples);
    for _i in 0..samples {
        // if i % 2000 == 0 {
        //     println!("info kmeans samples {}", i);
        // }

        let (hand, community) = CardSet::sample2(rng, 2, street.visible_community());
        let equity = to_cdf(&get_equity(hand, community));
        points.push(equity);
    }

    // dedup
    points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    points.dedup();
    assert!(points.len() >= num_centroids);

    let mut centroids: Vec<EquityVector> =
        rand::seq::index::sample(rng, points.len(), num_centroids)
            .into_iter()
            .map(|i| points[i].clone())
            .collect();

    let mut db = U::new(bins);

    // k means iteration
    for iter in 0..KMEANS_ITERS {
        if iter % 100 == 0 {
            println!("info kmeans iter {}", iter);
        }

        db.reset();
        db.reserve(num_centroids);
        for (i, centroid) in centroids.iter().enumerate() {
            db.store(i as u64, centroid);
        }

        if iter % 10 == 0 {
            // within-cluster-sum-of-squares
            let mut cost = 0.0;
            for sample in points.iter() {
                let (_centroid, distance) = db.query(&sample);
                cost += distance.abs();
            }

            println!("info kmeans within_cluster_l1 {}", cost as f32);
        }

        // find closest centroid per point
        let mut new_centroids: Vec<Vec<usize>> = vec![vec![]; num_centroids];
        for (i, point) in points.iter().enumerate() {
            let (centroid, _distance) = db.query(&point);
            new_centroids[centroid as usize].push(i);
        }

        let mut changed = false;
        for (centroid, close_points) in new_centroids.iter().enumerate() {
            if close_points.is_empty() {
                // this can be bad
                continue;
            }

            let centroid = &mut centroids[centroid];

            // use median to minimize L1
            for k in 0..bins {
                let mut values: Vec<f32> = close_points.iter().map(|&p| points[p][k]).collect();
                let middle = values.len() / 2;
                values.select_nth_unstable_by(middle, f32::total_cmp);

                if (centroid[k] - values[middle]).abs() < 0.0001 {
                    changed = true;
                }

                centroid[k] = values[middle];
            }
        }

        if !changed {
            println!("info kmeans early exit");
            break;
        }
    }

    centroids
}

fn make_db(centroids: &Vec<EquityVector>, bins: usize) -> Box<U> {
    let mut db = Box::new(U::new(bins));
    db.reserve(centroids.len());
    for (i, centroid) in centroids.iter().enumerate() {
        db.store(i as u64, centroid);
    }

    db
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

versions!(PreflopGroupVersions, V1, V2, Elbow);

impl Versioned<PreflopGroupVersions> for PreflopGroup {
    fn build(version: &PreflopGroupVersions, path: &str) -> Self {
        let rng = &mut rand::rng();
        let index = SuitIndex::new(2, 0);
        let equity: sample::Preflop = try_load(&sample::SamplePreflopVersions::V1, path);

        match version {
            PreflopGroupVersions::V1 => {
                let centroids = kmeans(
                    10000,
                    10,
                    sample::BINS,
                    Street::Preflop,
                    rng,
                    |hand, _community| equity.equity(hand, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, sample::BINS),
                    centroids,
                    equity,
                    index,
                }
            }
            PreflopGroupVersions::V2 => {
                let groups = 32;
                let centroids = kmeans(
                    10000,
                    groups,
                    sample::BINS,
                    Street::Preflop,
                    rng,
                    |hand, _community| equity.equity(hand, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, sample::BINS),

                    centroids,
                    equity,
                    index,
                }
            }
            PreflopGroupVersions::Elbow => {
                for groups in [1, 2, 4, 8, 16, 32, 64, 128] {
                    println!("info preflop elbow {}", groups);
                    _ = kmeans(
                        10000,
                        groups,
                        sample::BINS,
                        Street::Preflop,
                        rng,
                        |hand, _community| equity.equity(hand, &index).to_vec(),
                    );
                }

                let groups = 1;
                let centroids = kmeans(
                    10000,
                    groups,
                    sample::BINS,
                    Street::Preflop,
                    rng,
                    |hand, _community| equity.equity(hand, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, sample::BINS),
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

    fn from_bytes(version: &PreflopGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();
        let equity: sample::Preflop = try_load(&sample::SamplePreflopVersions::V1, path);
        let index = SuitIndex::new(2, 0);

        match *version {
            PreflopGroupVersions::V1 => Self {
                db: make_db(&data.centroids, sample::BINS),
                centroids: data.centroids,
                equity,
                index,
            },
            PreflopGroupVersions::V2 => Self {
                db: make_db(&data.centroids, sample::BINS),
                centroids: data.centroids,
                equity,
                index,
            },
            PreflopGroupVersions::Elbow => {
                unimplemented!()
            }
        }
    }

    fn should_cache(version: &PreflopGroupVersions) -> bool {
        match *version {
            PreflopGroupVersions::V1 => true,
            PreflopGroupVersions::V2 => true,
            PreflopGroupVersions::Elbow => false,
        }
    }
}

impl Group for PreflopGroup {
    fn map(&self, hand: CardSet, _community: CardSet) -> usize {
        let equity = to_cdf(&self.equity.equity(hand, &self.index));
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

versions!(FlopGroupVersions, V1, V2, Elbow);

impl Versioned<FlopGroupVersions> for FlopGroup {
    fn build(version: &FlopGroupVersions, path: &str) -> Self {
        let rng = &mut rand::rng();
        let equity: texture::Flop = try_load(&texture::TextureFlopVersions::V1, path);
        let index = SuitIndex::new(2, 3);

        match version {
            FlopGroupVersions::V1 => {
                let centroids = kmeans(
                    10000,
                    10,
                    texture::BINS,
                    Street::Flop,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, texture::BINS),
                    centroids,
                    equity,
                    index,
                }
            }
            FlopGroupVersions::V2 => {
                let centroids = kmeans(
                    10000,
                    20,
                    texture::BINS,
                    Street::Flop,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, texture::BINS),
                    centroids,
                    equity,
                    index,
                }
            }
            FlopGroupVersions::Elbow => {
                for groups in [1, 2, 4, 8, 16, 32, 64, 128] {
                    println!("info flop elbow {}", groups);
                    _ = kmeans(
                        10000,
                        groups,
                        texture::BINS,
                        Street::Flop,
                        rng,
                        |hand, community| equity.equity(hand, community, &index).to_vec(),
                    );
                }

                let groups = 1;
                let centroids = kmeans(
                    10000,
                    groups,
                    texture::BINS,
                    Street::Flop,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, texture::BINS),
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

    fn from_bytes(version: &FlopGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();
        let equity: texture::Flop = try_load(&texture::TextureFlopVersions::V1, path);
        let index = SuitIndex::new(2, 3);

        match *version {
            FlopGroupVersions::V1 => Self {
                db: make_db(&data.centroids, texture::BINS),
                centroids: data.centroids,
                equity,
                index,
            },
            FlopGroupVersions::V2 => Self {
                db: make_db(&data.centroids, texture::BINS),
                centroids: data.centroids,
                equity,
                index,
            },
            FlopGroupVersions::Elbow => unimplemented!(),
        }
    }

    fn should_cache(version: &FlopGroupVersions) -> bool {
        match *version {
            FlopGroupVersions::V1 => true,
            FlopGroupVersions::V2 => true,
            FlopGroupVersions::Elbow => false,
        }
    }
}

impl Group for FlopGroup {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        let equity = to_cdf(&self.equity.equity(hand, community, &self.index));
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

versions!(TurnGroupVersions, V1, V2, Elbow);

impl Versioned<TurnGroupVersions> for TurnGroup {
    fn build(version: &TurnGroupVersions, path: &str) -> Self {
        let rng = &mut rand::rng();
        let equity: texture::Turn = try_load(&texture::TextureTurnVersions::V1, path);
        let index = SuitIndex::new(2, 4);
        match version {
            TurnGroupVersions::V1 => {
                let centroids = kmeans(
                    10000,
                    8,
                    texture::BINS,
                    Street::Turn,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, texture::BINS),
                    centroids,
                    equity,
                    index,
                }
            }
            TurnGroupVersions::V2 => {
                let centroids = kmeans(
                    10000,
                    8,
                    texture::BINS,
                    Street::Turn,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, texture::BINS),
                    centroids,
                    equity,
                    index,
                }
            }
            TurnGroupVersions::Elbow => {
                for groups in [1, 2, 4, 8, 16, 32, 64, 128] {
                    println!("info turn elbow {}", groups);
                    _ = kmeans(
                        10000,
                        groups,
                        texture::BINS,
                        Street::Turn,
                        rng,
                        |hand, community| equity.equity(hand, community, &index).to_vec(),
                    );
                }

                let groups = 1;
                let centroids = kmeans(
                    10000,
                    groups,
                    texture::BINS,
                    Street::Turn,
                    rng,
                    |hand, community| equity.equity(hand, community, &index).to_vec(),
                );

                Self {
                    db: make_db(&centroids, texture::BINS),
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

    fn from_bytes(version: &TurnGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();
        let equity: texture::Turn = try_load(&texture::TextureTurnVersions::V1, path);
        let index = SuitIndex::new(2, 4);

        match *version {
            TurnGroupVersions::V1 => Self {
                db: make_db(&data.centroids, texture::BINS),
                centroids: data.centroids,
                equity,
                index,
            },
            TurnGroupVersions::V2 => Self {
                db: make_db(&data.centroids, texture::BINS),
                centroids: data.centroids,
                equity,
                index,
            },
            TurnGroupVersions::Elbow => unimplemented!(),
        }
    }

    fn should_cache(version: &TurnGroupVersions) -> bool {
        match *version {
            TurnGroupVersions::V1 => true,
            TurnGroupVersions::V2 => true,
            TurnGroupVersions::Elbow => false,
        }
    }
}

impl Group for TurnGroup {
    fn map(&self, hand: CardSet, community: CardSet) -> usize {
        let equity = to_cdf(&self.equity.equity(hand, community, &self.index));
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

versions!(RiverGroupVersions, V1, V2, Elbow);

impl Versioned<RiverGroupVersions> for RiverGroup {
    fn build(version: &RiverGroupVersions, path: &str) -> Self {
        let rng = &mut rand::rng();
        let equity: texture::River = try_load(&texture::TextureRiverVersions::V1, path);
        let index = SuitIndex::new(2, 5);

        match *version {
            RiverGroupVersions::V1 => {
                let centroids = kmeans(10000, 8, 1, Street::River, rng, |hand, community| {
                    vec![equity.equity(hand, community, &index)]
                });

                Self {
                    db: make_db(&centroids, 1),
                    centroids,
                    equity,
                    index,
                }
            }
            RiverGroupVersions::V2 => {
                let centroids = kmeans(10000, 8, 1, Street::River, rng, |hand, community| {
                    vec![equity.equity(hand, community, &index)]
                });

                Self {
                    db: make_db(&centroids, 1),
                    centroids,
                    equity,
                    index,
                }
            }
            RiverGroupVersions::Elbow => {
                for groups in [1, 2, 4, 8, 16, 32, 64, 128] {
                    println!("info river elbow {}", groups);
                    _ = kmeans(10000, groups, 1, Street::River, rng, |hand, community| {
                        vec![equity.equity(hand, community, &index)]
                    });
                }

                let groups = 1;
                let centroids = kmeans(10000, groups, 1, Street::River, rng, |hand, community| {
                    vec![equity.equity(hand, community, &index)]
                });

                Self {
                    db: make_db(&centroids, 1),
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

    fn from_bytes(version: &RiverGroupVersions, path: &str, bytes: &[u8]) -> Self {
        let data: EquityData = postcard::from_bytes(bytes).unwrap();
        let equity: texture::River = try_load(&texture::TextureRiverVersions::V1, path);
        let index = SuitIndex::new(2, 5);

        match *version {
            RiverGroupVersions::V1 => Self {
                db: make_db(&data.centroids, 1),
                centroids: data.centroids,
                equity,
                index,
            },
            RiverGroupVersions::V2 => Self {
                db: make_db(&data.centroids, 1),
                centroids: data.centroids,
                equity,
                index,
            },
            RiverGroupVersions::Elbow => unimplemented!(),
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
