use std::{
    io::{Read, Write},
    path::Path,
};

use rayon::prelude::*;

use serde::{Deserialize, Serialize};

use smallvec::smallvec;

use poker_evaluator::Evaluator;
use poker_indexer::Indexer;

use crate::histogram::Histogram;

const BUCKETS: usize = 47;

pub fn build_strengths(evaluator: &Evaluator) -> Vec<u16> {
    let indexer = Indexer::new(vec![5, 2]);

    let mut strength = vec![0; indexer.count[1] as usize];
    for i in 0..indexer.count[0] {
        let board = indexer.unindex(i, 0)[0];

        let mut list = Vec::new();
        for a in 0..52 {
            for b in 0..52 {
                let hole = 1 << a | 1 << b;
                if a < b && (hole & board) == 0 {
                    list.push((
                        evaluator.evaluate(board | hole),
                        indexer.index(smallvec![board, hole]),
                        (a, b),
                    ));
                }
            }
        }

        list.sort();

        let mut used = vec![0; 52];

        let mut sum = 0;
        for x in list.chunk_by(|a, b| a.0 == b.0) {
            for &(_, _, (a, b)) in x {
                used[a as usize] += 1;
                used[b as usize] += 1;
                sum += 1;
            }

            for &(_, index, (a, b)) in x {
                strength[index as usize] = sum + 1 - used[a as usize] - used[b as usize];
            }

            for &(_, _, (a, b)) in x {
                used[a as usize] += 1;
                used[b as usize] += 1;
                sum += 1;
            }
        }
    }

    strength
}

pub fn generate_flop_histograms(strength: &Vec<u16>) -> Vec<Vec<u16>> {
    let mapper = Indexer::new(vec![5, 2]);

    let indexer = Indexer::new(vec![2, 3]);

    (0..indexer.count[1])
        .into_iter()
        .map(|index| {
            let val = indexer.unindex(index, 1);

            let cards = val[0];
            let board = val[1];

            let mut result = vec![0; BUCKETS];
            for a in 0..52 {
                for b in 0..52 {
                    let next = 1 << a | 1 << b;
                    if a < b && next & (cards | board) == 0 {
                        let i = mapper.index(smallvec![board | next, cards]) as usize;

                        let b = ((strength[i] as f32 / 2162.0) * BUCKETS as f32) as usize;

                        result[b] += 1;
                    }
                }
            }

            result
        })
        .collect()
}

pub fn generate_turn_histograms(strength: &Vec<u16>) -> Vec<Vec<u8>> {
    let mapper = Indexer::new(vec![5, 2]);

    let indexer = Indexer::new(vec![2, 4]);

    (0..indexer.count[1])
        .into_par_iter()
        .map(|index| {
            let val = indexer.unindex(index, 1);

            let cards = val[0];
            let board = val[1];

            let mut result = vec![0; BUCKETS];
            for c in 0..52 {
                if (1 << c) & (cards | board) == 0 {
                    let i = mapper.index(smallvec![board | 1 << c, cards]) as usize;

                    let b = ((strength[i] as f32 / 2162.0) * BUCKETS as f32) as usize;

                    result[b] += 1;
                }
            }

            result
        })
        .collect()
}

pub fn build_ochs_histograms(strength: &Vec<u16>) -> Vec<Histogram> {
    let indexer = Indexer::new(vec![5, 2]);

    let mapper = Indexer::new(vec![2]);

    let mut histograms = vec![Histogram::new(BUCKETS); mapper.count[0] as usize];
    for i in 0..indexer.count[1] {
        let val = indexer.unindex(i, 1);

        let cards = val[1];
        let board = val[0];

        let mut list: Vec<(u64, u64)> = (0..4)
            .map(|p| {
                (
                    cards >> 13 * p & ((1 << 13) - 1),
                    board >> 13 * p & ((1 << 13) - 1),
                )
            })
            .collect();

        list.sort();

        let mut r = 4;
        let mut x = 1;
        for chunk in list.chunk_by(|a, b| a == b) {
            let c = chunk.len() as u64;

            for k in 0..c {
                x *= r - k;
                x /= k + 1;
            }

            r -= c;
        }

        histograms[mapper.index(smallvec![cards]) as usize].put(
            ((strength[i as usize] as f32 / 2162.0) * BUCKETS as f32) as usize,
            x as f32,
        );
    }

    histograms.into_iter().map(|x| x.norm()).collect()
}

pub fn generate_river_histograms(evaluator: &Evaluator, ochs: &Vec<usize>) -> Vec<Histogram> {
    let mapper = Indexer::new(vec![2, 5]);

    let mut count = vec![0; ochs.iter().max().unwrap() + 1];
    for a in 0..52 {
        for b in 0..52 {
            if a < b {
                count[ochs[mapper.index(smallvec![1 << a | 1 << b]) as usize]] += 1;
            }
        }
    }

    let indexer = Indexer::new(vec![5, 2]);

    let mut histograms = vec![vec![0.0; count.len()]; mapper.count[1] as usize];
    for i in 0..indexer.count[0] {
        let board = indexer.unindex(i, 0)[0];

        let mut list = Vec::new();
        for a in 0..52 {
            for b in 0..52 {
                let hole = 1 << a | 1 << b;
                if a < b && (hole & board) == 0 {
                    list.push((
                        evaluator.evaluate(board | hole),
                        mapper.index(smallvec![hole, board]) as usize,
                        mapper.index(smallvec![hole]) as usize,
                        (a, b),
                    ));
                }
            }
        }

        list.sort();

        let mut used = vec![vec![0; 52]; count.len()];

        let mut sum = vec![0; count.len()];
        for x in list.chunk_by(|a, b| a.0 == b.0) {
            for &(_, _, hole, (a, b)) in x {
                used[ochs[hole]][a as usize] += 1;
                used[ochs[hole]][b as usize] += 1;
                sum[ochs[hole]] += 1;
            }

            for &(_, index, hole, (a, b)) in x {
                for k in 0..count.len() {
                    histograms[index][k] = (sum[k] + (ochs[hole] == k) as u32
                        - used[k][a as usize]
                        - used[k][b as usize]) as f32;
                }
            }

            for &(_, _, hole, (a, b)) in x {
                used[ochs[hole]][a as usize] += 1;
                used[ochs[hole]][b as usize] += 1;
                sum[ochs[hole]] += 1;
            }
        }

        for (_, index, hole, (a, b)) in list {
            for k in 0..count.len() {
                histograms[index][k] /=
                    (count[k] + (ochs[hole] == k) as u32 - used[k][a] - used[k][b]) as f32;
            }
        }
    }

    histograms.into_iter().map(|x| Histogram::from(x)).collect()
}

pub fn load<T: for<'d> Deserialize<'d>>(path: &String) -> T {
    let mut buffer = Vec::new();

    std::fs::File::open(path)
        .unwrap()
        .read_to_end(&mut buffer)
        .unwrap();

    bincode::deserialize(&buffer).unwrap()
}

pub fn save<T: Serialize>(path: &String, data: &T) {
    let buffer = bincode::serialize(data).unwrap();

    std::fs::File::create(path)
        .unwrap()
        .write_all(&buffer)
        .unwrap();
}

pub fn get<T: for<'d> Deserialize<'d> + Serialize>(path: &String, f: Box<dyn Fn() -> T>) -> T {
    if Path::new(path).exists() {
        load(path)
    } else {
        let data = f();

        save(path, &data);

        data
    }
}