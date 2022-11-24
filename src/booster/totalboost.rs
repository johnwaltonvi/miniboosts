//! This file defines `TotalBoost` based on the paper
//! "Totally Corrective Boosting Algorithms that Maximize the Margin"
//! by Warmuth et al.
//! 
use polars::prelude::*;


use crate::{
    Booster,
    WeakLearner,

    State,
    Classifier,
    CombinedHypothesis,

    SoftBoost,
};


/// Since we can regard TotalBoost as
/// a special case of SoftBoost (with capping param is 1.0),
/// so that we use it.
pub struct TotalBoost<F> {
    softboost: SoftBoost<F>,
}


impl<F> TotalBoost<F>
    where F: Classifier,
{
    /// initialize the `TotalBoost`.
    pub fn init(data: &DataFrame, target: &Series) -> Self {
        let softboost = SoftBoost::init(data, target)
            .nu(1.0);

        TotalBoost { softboost }
    }


    /// Returns a optimal value of the optimization problem LPBoost solves
    pub fn opt_val(&self) -> f64 {
        self.softboost.opt_val()
    }


    /// Set the tolerance parameter.
    pub fn tolerance(mut self, tol: f64) -> Self {
        self.softboost = self.softboost.tolerance(tol);
        self
    }
}


impl<F> Booster<F> for TotalBoost<F>
    where F: Classifier + Clone,
{
    fn preprocess<W>(
        &mut self,
        weak_learner: &W,
        data: &DataFrame,
        target: &Series,
    )
        where W: WeakLearner<Clf = F>
    {
        self.softboost.preprocess(weak_learner, data, target);
    }


    fn boost<W>(
        &mut self,
        weak_learner: &W,
        data: &DataFrame,
        target: &Series,
        iteration: usize,
    ) -> State
        where W: WeakLearner<Clf = F>
    {
        self.softboost.boost(weak_learner, data, target, iteration)
    }


    fn postprocess<W>(
        &mut self,
        weak_learner: &W,
        data: &DataFrame,
        target: &Series,
    ) -> CombinedHypothesis<F>
        where W: WeakLearner<Clf = F>
    {
        self.softboost.postprocess(weak_learner, data, target)
    }
}
