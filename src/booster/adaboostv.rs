//! Provides the `AdaBoost*` by Rätsch & Warmuth, 2005.
use crate::data_type::Sample;
use crate::booster::core::Booster;
use crate::base_learner::core::{Classifier, CombinedClassifier};
use crate::base_learner::core::BaseLearner;



/// Struct `AdaBoostV` has 4 parameters.
/// 
/// - `tolerance` is the gap parameter,
/// - `rho` is a guess of the optimal margin,
/// - `gamma` is the minimum edge over the past edges,
/// - `dist` is the distribution over training examples,
pub struct AdaBoostV {
    pub(crate) tolerance: f64,
    pub(crate) rho:       f64,
    pub(crate) gamma:     f64,
    pub(crate) dist:      Vec<f64>,
}


impl AdaBoostV {
    /// Initialize the `AdaBoostV<D, L>`.
    pub fn init(sample: &Sample) -> AdaBoostV {
        let m = sample.len();
        assert!(m != 0);
        let uni = 1.0 / m as f64;
        AdaBoostV {
            tolerance:   0.0,
            rho:         1.0,
            gamma:       1.0,
            dist:        vec![uni; m],
        }
    }


    /// `max_loop` returns the maximum iteration
    /// of the Adaboost to find a combined hypothesis
    /// that has error at most `eps`.
    pub fn max_loop(&self, eps: f64) -> usize {
        let m = self.dist.len();

        2 * ((m as f64).ln() / (eps * eps)) as usize
    }

    /// `update_params` updates `self.distribution`
    /// and determine the weight on hypothesis
    /// that the algorithm obtained at current iteration.
    fn update_params(&mut self, predictions: Vec<f64>, edge: f64) -> f64 {


        // Update edge & margin estimation parameters
        self.gamma = edge.min(self.gamma);
        self.rho   = self.gamma - self.tolerance;


        let weight = {
            let e = ((1.0 + edge) / (1.0 - edge)).ln() / 2.0;
            let m = ((1.0 + self.rho) / (1.0 - self.rho)).ln() / 2.0;

            e - m
        };


        // To prevent overflow, take the logarithm.
        for (d, yh) in self.dist.iter_mut().zip(predictions.iter()) {
            *d = d.ln() - weight * yh;
        }


        let m = self.dist.len();
        let mut indices = (0..m).collect::<Vec<usize>>();
        indices.sort_unstable_by(|&i, &j| {
            self.dist[i].partial_cmp(&self.dist[j]).unwrap()
        });


        let mut normalizer = self.dist[indices[0]];
        for i in indices.into_iter().skip(1) {
            let mut a = normalizer;
            let mut b = self.dist[i];
            if a < b {
                std::mem::swap(&mut a, &mut b);
            }

            normalizer = a + (1.0 + (b - a).exp()).ln();
        }

        for d in self.dist.iter_mut() {
            *d = (*d - normalizer).exp();
        }

        weight
    }
}


impl<C> Booster<C> for AdaBoostV
    where C: Classifier + Eq + PartialEq
{


    fn run<B>(&mut self, base_learner: &B, sample: &Sample, eps: f64)
        -> CombinedClassifier<C>
        where B: BaseLearner<Clf = C>,
    {
        // Initialize parameters
        let m   = sample.len();
        self.dist      = vec![1.0 / m as f64; m];
        self.tolerance = eps;

        let mut weighted_classifier = Vec::new();


        let max_loop = self.max_loop(eps);
        println!("max_loop: {max_loop}");

        for _t in 1..=max_loop {
            // Get a new hypothesis
            let h = base_learner.best_hypothesis(sample, &self.dist);


            // Each element in `predictions` is the product of
            // the predicted vector and the correct vector
            let predictions = sample.iter()
                .map(|ex| ex.label * h.predict(&ex.data))
                .collect::<Vec<f64>>();


            let edge = predictions.iter()
                .zip(self.dist.iter())
                .fold(0.0, |acc, (&yh, &d)| acc + yh * d);


            // If `h` predicted all the examples in `sample` correctly,
            // use it as the combined classifier.
            if edge.abs() >= 1.0 {
                let sgn = edge.signum();
                weighted_classifier = vec![(sgn, h)];
                println!("Break loop after: {_t} iterations");
                break;
            }


            // Compute the weight on the new hypothesis
            let weight = self.update_params(predictions, edge);
            weighted_classifier.push(
                (weight, h)
            );
        }

        CombinedClassifier {
            weighted_classifier
        }
    }
}

