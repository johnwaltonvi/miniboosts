//! This file defines `LPBoost` based on the paper
//! ``Boosting algorithms for Maximizing the Soft Margin''
//! by Warmuth et al.
//! 
use polars::prelude::*;

use super::lp_model::LPModel;

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


use std::cell::RefCell;



/// LPBoost struct.  
/// LPBoost is originally invented in this paper: [Linear Programming Boosting via Column Generation](https://www.researchgate.net/publication/220343627_Linear_Programming_Boosting_via_Column_Generation) by Ayhan Demiriz, Kristin P. Bennett, and John Shawe-Taylor.
/// The code is based on this paper: [Boosting algorithms for Maximizing the Soft Margin](https://proceedings.neurips.cc/paper/2007/file/cfbce4c1d7c425baf21d6b6f2babe6be-Paper.pdf) by Manfred K. Warmuth, Karen Glocer, and Gunnar Rätsch.
/// 
/// # Example
/// The following code shows a small example 
/// for running [`LPBoost`](LPBoost).  
/// See also:
/// - [`LPBoost::nu`]
/// - [`DTree`]
/// - [`DTreeClassifier`]
/// - [`CombinedHypothesis<F>`]
/// - [`DTree::max_depth`]
/// - [`DTree::criterion`]
/// - [`DataFrame`]
/// - [`Series`]
/// - [`DataFrame::shape`]
/// - [`CsvReader`]
/// 
/// [`LPBoost::nu`]: LPBoost::nu
/// [`DTree`]: crate::weak_learner::DTree
/// [`DTreeClassifier`]: crate::weak_learner::DTreeClassifier
/// [`CombinedHypothesis<F>`]: crate::hypothesis::CombinedHypothesis
/// [`DTree::max_depth`]: crate::weak_learner::DTree::max_depth
/// [`DTree::criterion`]: crate::weak_learner::DTree::criterion
/// [`DataFrame`]: polars::prelude::DataFrame
/// [`Series`]: polars::prelude::Series
/// [`DataFrame::shape`]: polars::prelude::DataFrame::shape
/// [`CsvReader`]: polars::prelude::CsvReader
/// 
/// 
/// ```no_run
/// use polars::prelude::*;
/// use miniboosts::prelude::*;
/// 
/// // Read the training data from the CSV file.
/// let mut data = CsvReader::from_path(path_to_csv_file)
///     .unwrap()
///     .has_header(true)
///     .finish()
///     .unwrap();
/// 
/// // Split the column corresponding to labels.
/// let target = data.drop_in_place(class_column_name).unwrap();
/// 
/// // Get the number of training examples.
/// let n_sample = data.shape().0 as f64;
/// 
/// // Initialize `LPBoost` and set the tolerance parameter as `0.01`.
/// // This means `booster` returns a hypothesis 
/// // whose soft margin objective value is differs at most `0.01`
/// // from the optimal one.
/// // Further, at the end of this chain,
/// // LPBoost calls `LPBoost::nu` to set the capping parameter 
/// // as `0.1 * n_sample`, which means that, 
/// // at most, `0.1 * n_sample` examples are regarded as outliers.
/// let booster = LPBoost::init(&data, &target)
///     .tolerance(0.01)
///     .nu(0.1 * n_sample);
/// 
/// // Set the weak learner with setting parameters.
/// let weak_learner = DecisionTree::init(&data, &target)
///     .max_depth(2)
///     .criterion(Criterion::Edge);
/// 
/// // Run `LPBoost` and obtain the resulting hypothesis `f`.
/// let f: CombinedHypothesis<DTreeClassifier> = booster.run(&weak_learner);
/// 
/// // Get the predictions on the training set.
/// let predictions: Vec<i64> = f.predict_all(&data);
/// 
/// // Calculate the training loss.
/// let training_loss = target.i64()
///     .unwrap()
///     .into_iter()
///     .zip(predictions)
///     .map(|(true_label, prediction) {
///         let true_label = true_label.unwrap();
///         if true_label == prediction { 0.0 } else { 1.0 }
///     })
///     .sum::<f64>()
///     / n_sample;
/// 
///
/// println!("Training Loss is: {training_loss}");
/// ```
pub struct LPBoost<'a, F> {
    // Training data
    data: &'a DataFrame,

    // Corresponding label
    target: &'a Series,


    // Distribution over examples
    dist: Vec<f64>,

    // min-max edge of the new hypothesis
    gamma_hat: f64,

    // Tolerance parameter
    tolerance: f64,


    // Number of examples
    n_sample: usize,


    // Capping parameter
    nu: f64,


    // GRBModel.
    lp_model: Option<RefCell<LPModel>>,


    classifiers: Vec<F>,
    weights: Vec<f64>,


    terminated: usize,
}


impl<'a, F> LPBoost<'a, F>
    where F: Classifier
{
    /// Initialize the `LPBoost`.
    pub fn init(data: &'a DataFrame, target: &'a Series) -> Self {
        let n_sample = data.shape().0;
        assert!(n_sample != 0);


        let uni = 1.0 / n_sample as f64;
        LPBoost {
            data,
            target,

            dist:      vec![uni; n_sample],
            gamma_hat: 1.0,
            tolerance: uni,
            n_sample,
            nu:        1.0,
            lp_model: None,

            classifiers: Vec::new(),
            weights: Vec::new(),


            terminated: usize::MAX,
        }
    }


    /// This method updates the capping parameter.
    /// This parameter must be in `[1, n_sample]`.
    pub fn nu(mut self, nu: f64) -> Self {
        let n_sample = self.n_sample as f64;
        assert!((1.0..=n_sample).contains(&nu));
        self.nu = nu;

        self
    }


    /// Initializes the LP solver.
    fn init_solver(&mut self) {
        let n_sample = self.data.shape().0 as f64;
        assert!((1.0..=n_sample).contains(&self.nu));

        let upper_bound = 1.0 / self.nu;

        let lp_model = RefCell::new(LPModel::init(self.n_sample, upper_bound));

        self.lp_model = Some(lp_model);
    }


    /// Set the tolerance parameter.
    /// LPBoost guarantees the `tolerance`-approximate solution to
    /// the soft margin optimization.  
    /// Default value is `1.0 / sample_size`.
    #[inline(always)]
    pub fn tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }


    /// Returns the terminated iteration.
    /// This method returns `usize::MAX` before the boosting step.
    #[inline(always)]
    pub fn terminated(&self) -> usize {
        self.terminated
    }


    /// This method updates `self.dist` and `self.gamma_hat`
    /// by solving a linear program
    /// over the hypotheses obtained in past steps.
    #[inline(always)]
    fn update_distribution_mut(&self, h: &F) -> f64
    {
        self.lp_model.as_ref()
            .unwrap()
            .borrow_mut()
            .update(self.data, self.target, h)
    }
}


impl<F> Booster<F> for LPBoost<'_, F>
    where F: Classifier + Clone,
{
    fn preprocess<W>(
        &mut self,
        _weak_learner: &W,
    )
        where W: WeakLearner<Hypothesis = F>
    {
        let n_sample = self.data.shape().0;
        let uni = 1.0_f64 / n_sample as f64;

        self.init_solver();

        self.n_sample = n_sample;
        self.dist = vec![uni; n_sample];
        self.gamma_hat = 1.0;
        self.classifiers = Vec::new();
        self.terminated = usize::MAX;
    }


    fn boost<W>(
        &mut self,
        weak_learner: &W,
        _iteration: usize,
    ) -> State
        where W: WeakLearner<Hypothesis = F>,
    {
        let h = weak_learner.produce(self.data, self.target, &self.dist);

        // Each element in `margins` is the product of
        // the predicted vector and the correct vector

        let ghat = self.target.i64()
            .expect("The target class is not a dtype of i64")
            .into_iter()
            .enumerate()
            .map(|(i, y)| y.unwrap() as f64 * h.confidence(self.data, i))
            .zip(self.dist.iter())
            .map(|(yh, &d)| d * yh)
            .sum::<f64>();

        self.gamma_hat = ghat.min(self.gamma_hat);


        let gamma_star = self.update_distribution_mut(&h);


        self.classifiers.push(h);

        if gamma_star >= self.gamma_hat - self.tolerance {
            self.terminated = self.classifiers.len();
            return State::Terminate;
        }

        // Update the distribution over the training examples.
        self.dist = self.lp_model.as_ref()
            .unwrap()
            .borrow()
            .distribution();

        State::Continue
    }


    fn postprocess<W>(
        &mut self,
        _weak_learner: &W,
    ) -> CombinedHypothesis<F>
        where W: WeakLearner<Hypothesis = F>
    {
        let clfs = self.lp_model.as_ref()
            .unwrap()
            .borrow()
            .weight()
            .zip(self.classifiers.clone())
            .filter(|(w, _)| *w != 0.0)
            .collect::<Vec<(f64, F)>>();


        CombinedHypothesis::from(clfs)
    }
}



impl<F> Logger for LPBoost<'_, F>
    where F: Classifier
{
    fn weights_on_hypotheses(&mut self) {
        self.weights = self.lp_model.as_ref()
            .unwrap()
            .borrow()
            .weight()
            .collect::<Vec<f64>>();
    }


    /// LPBoost optimizes the soft margin over the current hypotheses
    /// obtained from the weak-learner.
    fn objective_value(&self)
        -> f64
    {
        soft_margin_objective(
            self.data, self.target,
            &self.weights[..], &self.classifiers[..], self.nu
        )
    }


    fn prediction(&self, data: &DataFrame, i: usize) -> f64 {
        self.weights.iter()
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
