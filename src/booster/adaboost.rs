//! Provides `AdaBoost` by Freund & Schapire, 1995.
use polars::prelude::*;
use rayon::prelude::*;


use crate::{
    Booster,
    WeakLearner,
    State,
    Classifier,
    CombinedHypothesis
};

use crate::research::Logger;


/// Defines `AdaBoost`.
pub struct AdaBoost<'a, F> {
    data: &'a DataFrame,
    target: &'a Series,

    dist: Vec<f64>,
    tolerance: f64,

    weighted_classifiers: Vec<(f64, F)>,


    max_iter: usize,

    terminated: usize,
}


impl<'a, F> AdaBoost<'a, F> {
    /// Initialize the `AdaBoost`.
    /// This method just sets the parameter `AdaBoost` holds.
    pub fn init(data: &'a DataFrame, target: &'a Series) -> Self {
        assert!(!data.is_empty());

        let n_sample = data.shape().0;

        let uni = 1.0 / n_sample as f64;
        AdaBoost {
            dist: vec![uni; n_sample],
            tolerance: 1.0 / (n_sample as f64 + 1.0),

            weighted_classifiers: Vec::new(),

            data,
            target,

            max_iter: usize::MAX,

            terminated: usize::MAX,
        }
    }


    /// `max_loop` returns the maximum iteration
    /// of the `AdaBoost` to find a combined hypothesis
    /// that has error at most `eps`.
    /// After the `self.max_loop()` iterations,
    /// `AdaBoost` guarantees zero training error in terms of zero-one loss
    /// if the training examples are linearly separable.
    pub fn max_loop(&self) -> usize {
        let n_sample = self.data.shape().0 as f64;

        (n_sample.ln() / self.tolerance.powi(2)) as usize
    }


    /// Set the tolerance parameter.
    pub fn tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }


    /// Returns a weight on the new hypothesis.
    /// `update_params` also updates `self.dist`
    #[inline]
    fn update_params(
        &mut self,
        margins: Vec<f64>,
        edge: f64
    ) -> f64
    {
        let n_sample = self.data.shape().0;


        // Compute the weight on new hypothesis.
        // This is the returned value of this function.
        let weight = ((1.0 + edge) / (1.0 - edge)).ln() / 2.0;


        // To prevent overflow, take the logarithm.
        self.dist.par_iter_mut()
            .zip(margins)
            .for_each(|(d, p)| *d = d.ln() - weight * p);


        // Sort indices by ascending order
        let mut indices = (0..n_sample).into_par_iter()
            .collect::<Vec<usize>>();
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



        // Update self.dist
        self.dist.par_iter_mut()
            .for_each(|d| *d = (*d - normalizer).exp());


        weight
    }
}


impl<F> Booster<F> for AdaBoost<'_, F>
    where F: Classifier + Clone,
{
    fn preprocess<W>(
        &mut self,
        _weak_learner: &W,
    )
        where W: WeakLearner<Hypothesis = F>
    {
        // Initialize parameters
        let n_sample = self.data.shape().0;
        let uni = 1.0 / n_sample as f64;
        self.dist = vec![uni; n_sample];

        self.weighted_classifiers = Vec::new();


        self.max_iter = self.max_loop();
    }


    fn boost<W>(
        &mut self,
        weak_learner: &W,
        iteration: usize,
    ) -> State
        where W: WeakLearner<Hypothesis = F>,
    {
        if self.max_iter < iteration {
            return State::Terminate;
        }


        // Get a new hypothesis
        let h = weak_learner.produce(self.data, self.target, &self.dist);


        // Each element in `margins` is the product of
        // the predicted vector and the correct vector
        let margins = self.target.i64()
            .expect("The target class is not an dtype i64")
            .into_iter()
            .enumerate()
            .map(|(i, y)| (y.unwrap() as f64 * h.confidence(self.data, i)))
            .collect::<Vec<f64>>();


        let edge = margins.iter()
            .zip(&self.dist[..])
            .map(|(&yh, &d)| yh * d)
            .sum::<f64>();


        // If `h` predicted all the examples in `sample` correctly,
        // use it as the combined classifier.
        if edge.abs() >= 1.0 {
            self.terminated = iteration;
            self.weighted_classifiers = vec![(edge.signum(), h)];
            return State::Terminate;
        }


        // Compute the weight on the new hypothesis
        let weight = self.update_params(margins, edge);
        self.weighted_classifiers.push((weight, h));

        State::Continue
    }


    fn postprocess<W>(
        &mut self,
        _weak_learner: &W,
    ) -> CombinedHypothesis<F>
        where W: WeakLearner<Hypothesis = F>
    {
        CombinedHypothesis::from(self.weighted_classifiers.clone())
    }
}


impl<F> Logger for AdaBoost<'_, F>
    where F: Classifier
{
    /// AdaBoost optimizes the exp loss
    fn objective_value(&self) -> f64 {
        let n_sample = self.data.shape().0 as f64;

        self.target.i64()
            .expect("The target class is not a dtype i64")
            .into_iter()
            .map(|y| y.unwrap() as f64)
            .enumerate()
            .map(|(i, y)| (- y * self.prediction(self.data, i)).exp())
            .sum::<f64>()
            / n_sample
    }


    fn prediction(&self, data: &DataFrame, i: usize) -> f64 {
        self.weighted_classifiers.iter()
            .map(|(w, h)| w * h.confidence(data, i))
            .sum::<f64>()
    }


    fn logging<L>(
        &self,
        loss_function: &L,
        test_data: &DataFrame,
        test_target: &Series,
    ) -> (f64, f64, f64)
        where L: Fn(f64, f64) -> f64
    {
        let objval = self.objective_value();
        let train = self.loss(loss_function, self.data, self.target);
        let test = self.loss(loss_function, test_data, test_target);

        (objval, train, test)
    }
}


