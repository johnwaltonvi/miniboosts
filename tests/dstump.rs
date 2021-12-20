extern crate lycaon;

use std::collections::HashMap;

use lycaon::data_type::*;

use lycaon::base_learner::core::BaseLearner;
use lycaon::base_learner::dstump::DStump;


#[test]
fn dstump_new() {
    let dstump = DStump::new();
    assert_eq!(dstump.sample_size, 0);
    assert_eq!(dstump.feature_size, 0);
    assert_eq!(dstump.indices.len(), 0);
}


#[test]
fn dstump_init() {
    let examples = vec![
        Data::Dense(vec![  1.2, 0.5, -1.0,  2.0]),
        Data::Dense(vec![  0.1, 0.2,  0.3, -9.0]),
        Data::Dense(vec![-21.0, 2.0,  1.9,  7.1])
    ];
    let labels = vec![1.0, -1.0, 1.0];


    let sample = to_sample(examples, labels);
    let dstump = DStump::init(&sample);


    let ans = vec![
        vec![2, 1, 0],
        vec![1, 0, 2],
        vec![0, 1, 2],
        vec![1, 0, 2]
    ];


    assert_eq!(dstump.sample_size, 3);
    assert_eq!(dstump.feature_size, 4);
    assert_eq!(dstump.indices, ans);
}


#[test]
fn best_hypothesis() {
    let examples = vec![
        Data::Dense(vec![  1.2, 0.5, -1.0,  2.0]),
        Data::Dense(vec![  0.1, 0.2,  0.3, -9.0]),
        Data::Dense(vec![-21.0, 2.0,  1.9,  7.1])
    ];
    let labels = vec![1.0, -1.0, 1.0];


    let sample = to_sample(examples, labels);


    let dstump = DStump::init(&sample);

    let distribution = vec![1.0/3.0; 3];
    let h = dstump.best_hypothesis(&sample, &distribution);

    assert_eq!(h.predict(&sample[0].data), sample[0].label);
    assert_eq!(h.predict(&sample[1].data), sample[1].label);
    assert_eq!(h.predict(&sample[2].data), sample[2].label);


    let distribution = vec![0.7, 0.1, 0.2];
    let h = dstump.best_hypothesis(&sample, &distribution);
    assert_eq!(h.predict(&sample[0].data), sample[0].label);
    assert_eq!(h.predict(&sample[1].data), sample[1].label);
    assert_eq!(h.predict(&sample[2].data), sample[2].label);
}


#[test]
fn best_hypothesis_sparse() {
    let tuples: Vec<(usize, f64)> = vec![(1, 0.2), (3, -12.5), (8, -4.0), (9, 0.8)];

    let mut examples = vec![HashMap::new(); 10];

    for (i, v) in tuples {
        examples[i].insert(0, v);
    }
    let examples = examples.into_iter().map(|x| Data::Sparse(x)).collect::<Vec<Data<f64>>>();

    let mut labels = vec![1.0; 10];
    labels[3] = -1.0; labels[8] = -1.0;


    let sample = to_sample(examples, labels);


    let dstump = DStump::init(&sample);

    assert_eq!(dstump.sample_size, 10);
    assert_eq!(dstump.feature_size, 1);

    let distribution = vec![1.0/10.0; 10];
    let h = dstump.best_hypothesis(&sample, &distribution);

    assert_eq!(h.predict(&sample[0].data), sample[0].label);
    assert_eq!(h.predict(&sample[1].data), sample[1].label);
    assert_eq!(h.predict(&sample[2].data), sample[2].label);

}


