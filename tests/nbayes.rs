use miniboosts::{
    GaussianNB,
    WeakLearner,
    Classifier,
    Sample,
};


// Toy example  (o/x are the pos/neg examples)
// This partition is a decisiton tree for the unit prior.
// 
// 15|
//   |                   5
//   |                  -
//   |                               6
//   |                              -
// 10|       4
//   |      -           Mean(-)          1
//   |                 *                +
//   |
//   |                         0     Mean(+)
//  5|                        +     *
//   |                                       2
//   |                                      +
//   |            3
//   |           -
//   |__________________________________________
//  0            5           10            15
// 
// 


#[test]
fn naive_bayes_toy_test() {
    let target = vec![1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0];
    let sample = Sample::from_dense_columns(
        vec![
            ("x", vec![10.0, 14.0, 15.0, 5.0, 3.0, 8.0, 12.0]),
            ("y", vec![5.0, 8.0, 3.0, 1.0, 9.0, 13.0, 11.0]),
        ],
        target.clone(),
    )
    .unwrap();


    let nbayes = GaussianNB::init();

    let dist = vec![1.0/7.0; 7];

    let f = nbayes.produce(&sample, &dist[..]);

    println!("{f:?}");

    let predictions = f.predict_all(&sample);
    println!("Predictions: {predictions:?}");
    println!("True labels: {target:?}");
}
