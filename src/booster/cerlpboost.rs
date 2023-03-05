//! This file defines `CERLPBoost` based on the paper
//! "On the Equivalence of Weak Learnability and Linaer Separability:
//!     New Relaxations and Efficient Boosting Algorithms"
//! by Shai Shalev-Shwartz and Yoram Singer.
//! I named this algorithm `CERLPBoost`
//! since it is referred as `the Corrective version of CERLPBoost`
//! in "Entropy Regularized LPBoost" by Warmuth et al.
//!
use rayon::prelude::*;

use crate::{
    Sample,
    Booster,
    WeakLearner,

    State,
    Classifier,
    CombinedHypothesis,
    research::Research,
};

/// Corrective ERLPBoost struct.  
/// This algorithm is based on this paper:
/// [On the equivalence of weak learnability and linear separability: new relaxations and efficient boosting algorithms](https://link.springer.com/article/10.1007/s10994-010-5173-z)
/// by Shai Shalev-Shwartz and Yoram Singer.
/// 
/// # Example
/// The following code shows a small example 
/// for running [`CERLPBoost`](CERLPBoost).  
/// See also:
/// - [`CERLPBoost::nu`]
/// - [`DTree`]
/// - [`DTreeClassifier`]
/// - [`CombinedHypothesis<F>`]
/// 
/// [`CERLPBoost::nu`]: CERLPBoost::nu
/// [`DTree`]: crate::weak_learner::DTree
/// [`DTreeClassifier`]: crate::weak_learner::DTreeClassifier
/// [`CombinedHypothesis<F>`]: crate::hypothesis::CombinedHypothesis
/// 
/// 
/// ```no_run
/// use miniboosts::prelude::*;
/// 
/// // Read the training sample from the CSV file.
/// // We use the column named `class` as the label.
/// let has_header = true;
/// let mut sample = Sample::from_csv(path_to_csv_file, has_header)
///     .unwrap()
///     .set_target("class");
/// 
/// 
/// // Get the number of training examples.
/// let n_sample = sample.shape().0 as f64;
/// 
/// // Set the upper-bound parameter of outliers in `sample`.
/// // Here we assume that the outliers are at most 1% of `sample`.
/// let nu = 0.1 * n_sample;
/// 
/// // Initialize `CERLPBoost` and set the tolerance parameter as `0.01`.
/// // This means `booster` returns a hypothesis whose training error is
/// // less than `0.01` if the traing examples are linearly separable.
/// // Note that the default tolerance parameter is set as `1 / n_sample`,
/// // where `n_sample = sample.shape().0` is 
/// // the number of training examples in `sample`.
/// let booster = CERLPBoost::init(&sample)
///     .tolerance(0.01)
///     .nu(0.1 * n_sample);
/// 
/// // Set the weak learner with setting parameters.
/// let weak_learner = DecisionTree::init(&sample)
///     .max_depth(2)
///     .criterion(Criterion::Edge);
/// 
/// // Run `CERLPBoost` and obtain the resulting hypothesis `f`.
/// let f: CombinedHypothesis<DTreeClassifier> = booster.run(&weak_learner);
/// 
/// // Get the predictions on the training set.
/// let predictions: Vec<i64> = f.predict_all(&sample);
/// 
/// // Calculate the training loss.
/// let target = sample.target();
/// let training_loss = target.into_iter()
///     .zip(predictions)
///     .map(|(&y, fx) if y as i64 == fx { 0.0 } else { 1.0 })
///     .sum::<f64>()
///     / n_sample;
/// 
///
/// println!("Training Loss is: {training_loss}");
/// ```
pub struct CERLPBoost<'a, F> {
    // Training sample
    sample: &'a Sample,

    dist: Vec<f64>,
    // A regularization parameter defined in the paper
    eta: f64,

    tolerance: f64,
    nu: f64,

    // Optimal value (Dual problem)
    dual_optval: f64,

    classifiers: Vec<(F, f64)>,

    max_iter: usize,
    terminated: usize,
}

impl<'a, F> CERLPBoost<'a, F> {
    /// Initialize the `CERLPBoost`.
    pub fn init(sample: &'a Sample) -> Self {
        let n_sample = sample.shape().0;

        // Set uni as an uniform weight
        let uni = 1.0 / n_sample as f64;

        // Set tolerance, sub_tolerance
        let tolerance = uni;

        // Set regularization parameter
        let nu = 1.0;
        let eta = 2.0 * (n_sample as f64 / nu).ln() / tolerance;

        Self {
            sample,

            dist: vec![uni; n_sample],
            tolerance,
            eta,
            nu: 1.0,
            dual_optval: 1.0,

            classifiers: Vec::new(),

            max_iter: usize::MAX,
            terminated: usize::MAX,
        }
    }


    /// This method updates the capping parameter.
    pub fn nu(mut self, nu: f64) -> Self {
        let n_sample = self.dist.len() as f64;
        assert!((1.0..=n_sample).contains(&nu));
        self.nu = nu;

        self.regularization_param();

        self
    }


    /// Update tolerance parameter `tolerance`.
    #[inline(always)]
    pub fn tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance / 2.0;
        self
    }


    /// Compute the dual objective value
    #[inline(always)]
    fn dual_objval_mut(&mut self)
        where F: Classifier + PartialEq,
    {
        self.dual_optval = self.classifiers.iter()
            .map(|(h, _)| {
                self.sample.target()
                    .into_iter()
                    .zip(self.dist.iter().copied())
                    .enumerate()
                    .map(|(i, (y, d))|
                        d * y * h.confidence(self.sample, i)
                    )
                    .sum::<f64>()
            })
            .reduce(f64::max)
            .unwrap();
    }


    /// Returns an optimal value of the dual problem.
    /// This value is an `self.tolerance`-accurate value of the primal one.
    pub fn opt_val(&self) -> f64 {
        self.dual_optval
    }


    /// Update regularization parameter.
    /// (the regularization parameter on
    ///  `self.tolerance` and `self.nu`.)
    #[inline(always)]
    fn regularization_param(&mut self) {
        let m = self.dist.len() as f64;
        let ln_part = (m / self.nu).ln();
        self.eta = ln_part / self.tolerance;
    }


    /// returns the maximum iteration of the CERLPBoost
    /// to find a combined hypothesis that has error at most `tolerance`.
    pub fn max_loop(&mut self) -> usize {

        let m = self.dist.len() as f64;

        let ln_m = (m / self.nu).ln();
        let max_iter = 8.0 * ln_m / self.tolerance.powi(2);

        max_iter.ceil() as usize
    }
}


impl<F> CERLPBoost<'_, F>
    where F: Classifier + PartialEq,
{
    /// Updates weight on hypotheses and `self.dist` in this order.
    fn update_distribution_mut(&mut self)
    {
        self.dist
            .iter_mut()
            .zip(self.sample.target().into_iter())
            .enumerate()
            .for_each(|(i, (d, y))| {
                let p = prediction(i, self.sample, &self.classifiers[..]);
                *d = - self.eta * y * p
            });

        let n_sample = self.sample.shape().0;
        // Sort the indices over `self.dist` in non-increasing order.
        let mut indices = (0..n_sample).collect::<Vec<_>>();
        indices.sort_by(|&i, &j|
            self.dist[j].partial_cmp(&self.dist[i]).unwrap()
        );

        let logsums = indices.iter()
            .rev()
            .fold(Vec::with_capacity(n_sample), |mut vec, &i| {
                // TODO use `get_unchecked`
                let temp = match vec.last() {
                    None => self.dist[i],
                    Some(&val) => {
                        let mut a = val;
                        let mut b = self.dist[i];
                        if a < b {
                            std::mem::swap(&mut a, &mut b)
                        };

                        a + (1.0 + (b - a).exp()).ln()
                    }
                };
                vec.push(temp);
                vec
            })
            .into_iter()
            .rev();

        let ub = 1.0 / self.nu;
        let log_cap = self.nu.ln();

        let mut idx_with_logsum = indices.into_iter().zip(logsums).enumerate();

        while let Some((i, (i_sorted, logsum))) = idx_with_logsum.next() {
            let log_xi = (1.0 - ub * i as f64).ln() - logsum;
            // TODO replace this line into `get_unchecked`
            let d = self.dist[i_sorted];

            // Stopping criterion of this while loop
            if log_xi + d + log_cap <= 0.0 {
                self.dist[i_sorted] = (log_xi + d).exp();
                while let Some((_, (ii, _))) = idx_with_logsum.next() {
                    self.dist[ii] = (log_xi + self.dist[ii]).exp();
                }
                break;
            }

            self.dist[i_sorted] = ub;
        }
    }

    /// Update the weights on hypotheses
    fn update_clf_weight_mut(&mut self, new_clf: F, gap_vec: Vec<f64>)
    {
        // Numerator
        let numer = gap_vec
            .iter()
            .zip(self.dist.iter())
            .fold(0.0, |acc, (&v, &d)| acc + v * d);

        let squared_inf_norm = gap_vec
            .into_iter()
            .fold(f64::MIN, |acc, v| acc.max(v.abs()))
            .powi(2);

        // Denominator
        let denom = self.eta * squared_inf_norm;

        // Name the weight on new hypothesis as `weight`
        let weight = 0.0_f64.max(1.0_f64.min(numer / denom));

        let mut already_exist = false;
        for (clf, w) in self.classifiers.iter_mut() {
            if *clf == new_clf {
                already_exist = true;
                *w += weight;
            } else {
                *w *= 1.0 - weight;
            }
        }

        if !already_exist {
            self.classifiers.push((new_clf, weight));
        }
    }
}

impl<F> Booster<F> for CERLPBoost<'_, F>
    where F: Classifier + Clone + PartialEq + std::fmt::Debug,
{
    fn preprocess<W>(
        &mut self,
        _weak_learner: &W,
    )
        where W: WeakLearner<Hypothesis = F>
    {
        let n_sample = self.sample.shape().0;
        let uni = 1.0 / n_sample as f64;

        self.dist = vec![uni; n_sample];


        assert!((0.0..1.0).contains(&self.tolerance));
        self.regularization_param();
        self.max_iter = self.max_loop();
        self.terminated = self.max_iter;

        self.classifiers = Vec::new();
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

        // Update the distribution over examples
        self.update_distribution_mut();

        // Receive a hypothesis from the base learner
        let h = weak_learner.produce(self.sample, &self.dist);

        // println!("h: {h:?}");

        let gap_vec = self.sample.target()
            .into_iter()
            .enumerate()
            .map(|(i, y)| {
                let old_pred = prediction(
                    i, self.sample, &self.classifiers[..]
                );
                let new_pred = h.confidence(self.sample, i);

                y * (new_pred - old_pred)
            })
            .collect::<Vec<_>>();

        // Compute the difference between the new hypothesis
        // and the current combined hypothesis
        let diff = gap_vec.par_iter()
            .zip(&self.dist[..])
            .map(|(v, d)| v * d)
            .sum::<f64>();

        // Update the parameters
        if diff <= self.tolerance {
            self.terminated = iteration;
            return State::Terminate;
        }

        // Update the weight on hypotheses
        self.update_clf_weight_mut(h, gap_vec);

        State::Continue
    }


    fn postprocess<W>(
        &mut self,
        _weak_learner: &W,
    ) -> CombinedHypothesis<F>
        where W: WeakLearner<Hypothesis = F>
    {
        // Compute the dual optimal value for debug
        self.dual_objval_mut();

        let weighted_classifier = self.classifiers.clone()
            .into_iter()
            .filter_map(|(h, w)| if w != 0.0 { Some((w, h)) } else { None })
            .collect::<Vec<_>>();

        CombinedHypothesis::from(weighted_classifier)
    }
}

fn prediction<F>(
    i: usize,
    sample: &Sample,
    classifiers: &[(F, f64)]
) -> f64
    where F: Classifier,
{
    classifiers.iter()
        .map(|(h, w)| w * h.confidence(sample, i))
        .sum()
}


impl<H> Research<H> for CERLPBoost<'_, H>
    where H: Classifier + Clone,
{
    fn current_hypothesis(&self) -> CombinedHypothesis<H> {
        let total_weight = self.classifiers.iter()
            .map(|(_, w)| w.abs())
            .sum::<f64>();

        let f = self.classifiers.clone()
            .into_iter()
            .filter_map(|(h, w)|
                if w != 0.0 { Some((w / total_weight, h)) } else { None }
            )
            .collect::<Vec<_>>();

        CombinedHypothesis::from(f)
    }
}
