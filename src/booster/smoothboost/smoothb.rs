//! This file defines `SmoothBoost` based on the paper
//! ``Smooth Boosting and Learning with Malicious Noise''
//! by Rocco A. Servedio.


use polars::prelude::*;
use rayon::prelude::*;

use crate::{
    Booster,
    WeakLearner,

    State,
    Classifier,
    CombinedHypothesis,
};


use crate::research::{
    Logger,
    soft_margin_objective,
};


/// SmoothBoost. See Figure 1
/// in [this paper](https://www.jmlr.org/papers/volume4/servedio03a/servedio03a.pdf).
pub struct SmoothBoost<'a, F> {
    data: &'a DataFrame,
    target: &'a Series,

    /// Desired accuracy
    kappa: f64,

    /// Desired margin for the final hypothesis.
    /// To guarantee the convergence rate, `theta` should be
    /// `gamma / (2.0 + gamma)`.
    theta: f64,

    /// Weak-learner guarantee;
    /// for any distribution over the training examples,
    /// the weak-learner returns a hypothesis
    /// with edge at least `gamma`.
    gamma: f64,

    /// The number of training examples.
    n_sample: usize,

    current: usize,

    /// Terminated iteration.
    terminated: usize,

    max_iter: usize,

    classifiers: Vec<F>,


    m: Vec<f64>,
    n: Vec<f64>,
}


impl<'a, F> SmoothBoost<'a, F> {
    /// Initialize `SmoothBoost`.
    pub fn init(data: &'a DataFrame, target: &'a Series) -> Self {
        let n_sample = data.shape().0;

        let gamma = 0.5;


        Self {
            data,
            target,

            kappa: 0.5,
            theta: gamma / (2.0 + gamma), // gamma / (2.0 + gamma)
            gamma,

            n_sample,

            current: 0_usize,

            terminated: usize::MAX,
            max_iter: usize::MAX,

            classifiers: Vec::new(),

            m: Vec::new(),
            n: Vec::new(),
        }
    }


    /// Set the parameter `kappa`.
    #[inline(always)]
    pub fn tolerance(mut self, kappa: f64) -> Self {
        self.kappa = kappa;

        self
    }


    /// Set the parameter `gamma`.
    #[inline(always)]
    pub fn gamma(mut self, gamma: f64) -> Self {
        self.gamma = gamma;

        self
    }


    /// Set the parameter `theta`.
    fn theta(&mut self) {
        self.theta = self.gamma / (2.0 + self.gamma);
    }


    /// Returns the maximum iteration
    /// of SmoothBoost to satisfy the stopping criterion.
    fn max_loop(&self) -> usize {
        let denom = self.kappa
            * self.gamma.powi(2)
            * (1.0 - self.gamma).sqrt();


        (2.0 / denom).ceil() as usize
    }


    fn check_preconditions(&self) {
        // Check `kappa`.
        if !(0.0..1.0).contains(&self.kappa) || self.kappa <= 0.0 {
            panic!(
                "Invalid kappa. \
                 The parameter `kappa` must be in (0.0, 1.0)"
            );
        }

        // Check `gamma`.
        if !(self.theta..0.5).contains(&self.gamma) {
            panic!(
                "Invalid gamma. \
                 The parameter `gamma` must be in [self.theta, 0.5)"
            );
        }
    }
}



impl<F> Booster<F> for SmoothBoost<'_, F>
    where F: Classifier + Clone,
{
    fn preprocess<W>(
        &mut self,
        _weak_learner: &W,
    )
        where W: WeakLearner<Hypothesis = F>
    {
        self.n_sample = self.data.shape().0;
        // Set the paremeter `theta`.
        self.theta();

        // Check whether the parameter satisfies the pre-conditions.
        self.check_preconditions();


        self.current = 0_usize;
        self.max_iter = self.max_loop();
        self.terminated = self.max_iter;

        self.classifiers = Vec::new();


        self.m = vec![1.0; self.n_sample];
        self.n = vec![1.0; self.n_sample];
    }


    fn boost<W>(
        &mut self,
        weak_learner: &W,
        iteration: usize,
    ) -> State
        where W: WeakLearner<Hypothesis = F>
    {

        if self.max_iter < iteration {
            return State::Terminate;
        }

        self.current = iteration;


        let sum = self.m.iter().sum::<f64>();
        // Check the stopping criterion.
        if sum < self.n_sample as f64 * self.kappa {
            self.terminated = iteration - 1;
            return State::Terminate;
        }


        // Compute the distribution.
        let dist = self.m.iter()
            .map(|mj| *mj / sum)
            .collect::<Vec<_>>();


        // Call weak learner to obtain a hypothesis.
        self.classifiers.push(
            weak_learner.produce(self.data, self.target, &dist[..])
        );
        let h: &F = self.classifiers.last().unwrap();


        let margins = self.target.i64()
            .expect("The target is not a dtype i64")
            .into_iter()
            .enumerate()
            .map(|(i, y)| y.unwrap() as f64 * h.confidence(self.data, i));


        // Update `n`
        self.n.iter_mut()
            .zip(margins)
            .for_each(|(nj, yh)| {
                *nj = *nj + yh - self.theta;
            });


        // Update `m`
        self.m.par_iter_mut()
            .zip(&self.n[..])
            .for_each(|(mj, nj)| {
                if *nj <= 0.0 {
                    *mj = 1.0;
                } else {
                    *mj = (1.0 - self.gamma).powf(*nj * 0.5);
                }
            });

        State::Continue
    }


    fn postprocess<W>(
        &mut self,
        _weak_learner: &W,
    ) -> CombinedHypothesis<F>
        where W: WeakLearner<Hypothesis = F>
    {
        let weight = 1.0 / self.terminated as f64;
        let clfs = self.classifiers.clone()
            .into_iter()
            .map(|h| (weight, h))
            .collect::<Vec<(f64, F)>>();

        CombinedHypothesis::from(clfs)
    }
}


impl<F> Logger for SmoothBoost<'_, F>
    where F: Classifier
{
    /// AdaBoost optimizes the exp loss
    fn objective_value(&self)
        -> f64
    {
        let unit = if self.current > 0 {
            1.0 / self.current as f64
        } else {
            0.0
        };
        let weights = vec![unit; self.current];


        let n_sample = self.data.shape().0 as f64;
        let nu = self.kappa * n_sample;

        soft_margin_objective(
            self.data, self.target, &weights[..], &self.classifiers[..], nu
        )
    }


    fn prediction(&self, data: &DataFrame, i: usize) -> f64 {
        let unit = if self.current > 0 {
            1.0 / self.current as f64
        } else {
            0.0
        };
        let weights = vec![unit; self.current];

        weights.iter()
            .zip(&self.classifiers[..])
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
