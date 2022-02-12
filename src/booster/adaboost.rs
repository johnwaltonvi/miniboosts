//! Provides the `AdaBoost` by Freund & Schapire, 1995.
use crate::{Data, Sample};
use crate::{Classifier, CombinedClassifier};
use crate::BaseLearner;
use crate::Booster;


/// Struct `AdaBoost` has one parameter.
/// 
/// - `dist` is the distribution over training examples,
pub struct AdaBoost {
    pub(crate) dist: Vec<f64>,
}


impl AdaBoost {
    /// Initialize the `AdaBoost`.
    /// This method just sets the parameter `AdaBoost` holds.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use lycaon::{Sample, AdaBoost};
    /// 
    /// let examples = vec![
    ///     vec![1.0, 2.0, 3.0],
    ///     vec![4.0, 5.0, 6.0],
    /// ];
    /// let labels = vec![1.0, -1.0];
    /// 
    /// let sample = Sample::from((examples, labels));
    /// 
    /// let booster = AdaBoost::init(&sample);
    /// ```
    pub fn init<T: Data>(sample: &Sample<T>) -> AdaBoost {
        let m = sample.len();
        assert!(m != 0);
        let uni = 1.0 / m as f64;
        AdaBoost {
            dist: vec![uni; m],
        }
    }


    /// `max_loop` returns the maximum iteration
    /// of the `AdaBoost` to find a combined hypothesis
    /// that has error at most `eps`.
    /// After the `self.max_loop()` iterations,
    /// `AdaBoost` guarantees no miss-classification on `sample<T>`
    /// if the training examples are linearly separable.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use lycaon::{Sample, AdaBoost};
    /// 
    /// let examples = vec![
    ///     vec![1.0, 2.0, 3.0],
    ///     vec![4.0, 5.0, 6.0],
    /// ];
    /// let labels = vec![1.0, -1.0];
    /// 
    /// let sample = Sample::from((examples, labels));
    /// 
    /// let booster = AdaBoost::init(&sample);
    /// let eps = 0.01_f64;
    /// 
    /// let expected = (sample.len() as f64).ln() / eps.powi(2);
    /// 
    /// assert_eq!(booster.max_loop(eps), expected as u64);
    /// ```
    /// 
    pub fn max_loop(&self, eps: f64) -> u64 {
        let m = self.dist.len();

        ((m as f64).ln() / (eps * eps)) as u64
    }


    /// Returns a weight on the new hypothesis.
    /// `update_params` also updates `self.distribution`
    #[inline]
    fn update_params(&mut self,
                     predictions: Vec<f64>,
                     edge:        f64)
        -> f64
    {
        let m = self.dist.len();


        // Compute the weight on new hypothesis.
        // This is the returned value of this function.
        let weight = ((1.0 + edge) / (1.0 - edge)).ln() / 2.0;


        // To prevent overflow, take the logarithm.
        for (d, p) in self.dist.iter_mut().zip(predictions.iter()) {
            *d = d.ln() - weight * p;
        }


        // Sort indices by ascending order
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


        // Update the distribution
        for d in self.dist.iter_mut() {
            *d = (*d - normalizer).exp();
        }


        weight
    }
}


impl<D, C> Booster<D, C> for AdaBoost
    where D: Data,
          C: Classifier<D> + Eq + PartialEq,
{
    fn run<B>(&mut self, 
              base_learner: &B,
              sample:       &Sample<D>,
              eps:          f64)
        -> CombinedClassifier<D, C>
        where B: BaseLearner<D, Clf = C>,
    {
        // Initialize parameters
        let m   = sample.len();
        let uni = 1.0 / m as f64;
        self.dist = vec![uni; m];

        let mut weighted_classifier = Vec::new();


        let max_loop = self.max_loop(eps);
        println!("max_loop: {max_loop}");

        for _t in 1..=max_loop {
            // Get a new hypothesis
            let h = base_learner.best_hypothesis(sample, &self.dist);


            // Each element in `predictions` is the product of
            // the predicted vector and the correct vector
            let predictions = sample.iter()
                .map(|(dat, lab)| *lab * h.predict(dat))
                .collect::<Vec<f64>>();


            let edge = predictions.iter()
                .zip(self.dist.iter())
                .fold(0.0, |acc, (&yh, &d)| acc + yh * d);


            // If `h` predicted all the examples in `sample` correctly,
            // use it as the combined classifier.
            if edge >= 1.0 {
                weighted_classifier = vec![(1.0, h)];
                println!("Break loop after: {_t} iterations");
                break;
            }


            // Compute the weight on the new hypothesis
            let weight = self.update_params(predictions, edge);
            weighted_classifier.push(
                (weight, h)
            );
        }

        CombinedClassifier::from(weighted_classifier)

    }
}

