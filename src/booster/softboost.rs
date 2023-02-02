//! This file defines `SoftBoost` based on the paper
//! "Boosting Algorithms for Maximizing the Soft Margin"
//! by Warmuth et al.
//! 
use polars::prelude::*;
// use rayon::prelude::*;


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

use grb::prelude::*;



/// Struct `SoftBoost` has 3 main parameters.
/// 
/// - `dist` is the distribution over training examples,
pub struct SoftBoost<'a, F> {
    data: &'a DataFrame,
    target: &'a Series,

    pub(crate) dist: Vec<f64>,

    // `gamma_hat` corresponds to $\min_{q=1, .., t} P^q (d^{q-1})
    gamma_hat: f64,
    tolerance: f64,
    // an accuracy parameter for the sub-problems
    sub_tolerance: f64,
    nu: f64,

    env: Env,


    classifiers: Vec<F>,


    max_iter: usize,
    terminated: usize,


    weights: Vec<f64>,
}


impl<'a, F> SoftBoost<'a, F>
    where F: Classifier
{
    /// Initialize the `SoftBoost`.
    pub fn init(data: &'a DataFrame, target: &'a Series) -> Self {
        let n_sample = data.shape().0;
        assert!(n_sample != 0);

        let mut env = Env::new("").unwrap();

        env.set(param::OutputFlag, 0).unwrap();

        // Set uni as an uniform weight
        let uni = 1.0 / n_sample as f64;

        let dist = vec![uni; n_sample];


        // Set tolerance, sub_tolerance
        let tolerance = uni;


        // Set gamma_hat
        let gamma_hat = 1.0;


        SoftBoost {
            data,
            target,

            dist,
            gamma_hat,
            tolerance,
            sub_tolerance: 1e-6,
            nu: 1.0,
            env,

            classifiers: Vec::new(),
            weights: Vec::new(),

            max_iter: usize::MAX,
            terminated: usize::MAX,
        }
    }


    /// This method updates the capping parameter.
    #[inline(always)]
    pub fn nu(mut self, nu: f64) -> Self {
        assert!(
            1.0 <= nu
            &&
            nu <= self.dist.len() as f64
        );
        self.nu = nu;

        self
    }


    /// Set the tolerance parameter.
    #[inline(always)]
    pub fn tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }


    /// `max_loop` returns the maximum iteration
    /// of the Adaboost to find a combined hypothesis
    /// that has error at most `tolerance`.
    pub fn max_loop(&mut self) -> usize {

        let m = self.dist.len() as f64;

        let temp = (m / self.nu).ln();
        let max_iter = 2.0 * temp / self.tolerance.powi(2);

        max_iter.ceil() as usize
    }


    /// Returns a optimal value of the optimization problem LPBoost solves
    pub fn opt_val(&self) -> f64 {
        self.gamma_hat
    }
}


impl<F> SoftBoost<'_, F>
    where F: Classifier,
{
    /// Set the weight on the classifiers.
    /// This function is called at the end of the boosting.
    fn set_weights(&self)
        -> std::result::Result<Vec<f64>, grb::Error>
    {
        let mut model = Model::with_env("", &self.env)?;

        let m = self.dist.len();
        let t = self.classifiers.len();

        // Initialize GRBVars
        let wt_vec = (0..t).map(|i| {
                let name = format!("w[{i}]");
                add_ctsvar!(model, name: &name, bounds: 0_f64..).unwrap()
            }).collect::<Vec<_>>();
        let xi_vec = (0..m).map(|i| {
                let name = format!("xi[{i}]");
                add_ctsvar!(model, name: &name, bounds: 0.0_f64..).unwrap()
            }).collect::<Vec<_>>();
        let rho = add_ctsvar!(model, name: "rho", bounds: ..)?;


        // Set constraints
        let iter = self.target.i64()
            .expect("The target class is not a dtype i64")
            .into_iter()
            .zip(xi_vec.iter())
            .enumerate();

        for (i, (y, &xi)) in iter {
            let y = y.unwrap() as f64;
            let expr = wt_vec.iter()
                .zip(&self.classifiers[..])
                .map(|(&w, h)| w * h.confidence(self.data, i))
                .grb_sum();
            let name = format!("sample[{i}]");
            model.add_constr(&name, c!(y * expr >= rho - xi))?;
        }

        model.add_constr(
            "sum_is_1", c!(wt_vec.iter().grb_sum() == 1.0)
        )?;
        model.update()?;


        // Set the objective function
        let param = 1.0 / self.nu;
        let objective = rho - param * xi_vec.iter().grb_sum();
        model.set_objective(objective, Maximize)?;
        model.update()?;


        model.optimize()?;


        let status = model.status()?;

        if status != Status::Optimal {
            panic!("Cannot solve the primal problem. Status: {status:?}");
        }


        // Assign weights over the hypotheses
        let weights = wt_vec.into_iter()
            .map(|w| model.get_obj_attr(attr::X, &w).unwrap())
            .collect::<Vec<_>>();

        Ok(weights)
    }


    /// Updates `self.dist`
    /// Returns `None` if the stopping criterion satisfied.
    fn update_params_mut(&mut self) -> Option<()> {
        loop {
            // Initialize GRBModel
            let mut model = Model::with_env("", &self.env).unwrap();


            // Set variables that are used in the optimization problem
            let cap = 1.0 / self.nu;

            let vars = self.dist.iter()
                .copied()
                .enumerate()
                .map(|(i, d)| {
                    let lb = - d;
                    let ub = cap - d;
                    let name = format!("delta[{i}]");
                    add_ctsvar!(model, name: &name, bounds: lb..ub)
                        .unwrap()
                })
                .collect::<Vec<Var>>();
            model.update().unwrap();


            // Set constraints
            self.classifiers.iter()
                .enumerate()
                .for_each(|(j, h)| {
                    let iter = self.target.i64()
                        .expect("The target class is not a dtype i64");
                    let expr = vars.iter()
                        .zip(self.dist.iter().copied())
                        .zip(iter)
                        .enumerate()
                        .map(|(i, ((v, d), y))| {
                            let y = y.unwrap() as f64;
                            let p = h.confidence(self.data, i);
                            y * p * (d + *v)
                        })
                        .grb_sum();

                    let name = format!("h[{j}]");
                    model.add_constr(
                        &name, c!(expr <= self.gamma_hat - self.tolerance)
                    ).unwrap();
                });


            model.add_constr(
                "zero_sum", c!(vars.iter().grb_sum() == 0.0)
            ).unwrap();
            model.update().unwrap();


            // Set objective function
            let m = self.dist.len() as f64;
            let objective = self.dist.iter()
                .zip(vars.iter())
                .map(|(&d, &v)| {
                    let lin_coef = (m * d).ln() + 1.0;
                    lin_coef * v + (v * v) * (1.0 / (2.0 * d))
                })
                .grb_sum();

            model.set_objective(objective, Minimize).unwrap();
            model.update().unwrap();


            // Optimize
            model.optimize().unwrap();


            // Check the status
            let status = model.status().unwrap();
            // If the status is `Status::Infeasible`,
            // it implies that a `tolerance`-optimality
            // of the previous solution
            if status == Status::Infeasible || status == Status::InfOrUnbd {
                return None;
            }


            // At this point, the status is not `Status::Infeasible`.
            // If the status is not `Status::Optimal`, something wrong.
            if status != Status::Optimal {
                panic!("Status is {status:?}. something wrong.");
            }



            // Check the stopping criterion
            let mut l2 = 0.0;
            for (v, d) in vars.iter().zip(self.dist.iter_mut()) {
                let val = model.get_obj_attr(attr::X, v).unwrap();
                *d += val;
                l2 += val * val;
            }
            let l2 = l2.sqrt();

            if l2 < self.sub_tolerance {
                break;
            }
        }


        if self.dist.iter().any(|&d| d == 0.0) {
            return None;
        }


        Some(())
    }
}


impl<F> Booster<F> for SoftBoost<'_, F>
    where F: Classifier + Clone,
{
    fn preprocess<W>(
        &mut self,
        _weak_learner: &W,
    )
        where W: WeakLearner<Hypothesis = F>
    {
        let n_sample = self.data.shape().0;

        let uni = 1.0 / n_sample as f64;

        self.dist = vec![uni; n_sample];

        self.sub_tolerance = self.tolerance / 10.0;

        self.max_iter = self.max_loop();
        self.terminated = self.max_iter;
        self.classifiers = Vec::new();

        self.gamma_hat = 1.0;
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

        // Receive a hypothesis from the base learner
        let h = weak_learner.produce(self.data, self.target, &self.dist);

        // update `self.gamma_hat`
        let edge = self.target.i64()
            .expect("The target class is not a dtype i64")
            .into_iter()
            .zip(self.dist.iter().copied())
            .enumerate()
            .map(|(i, (y, d))|
                d * y.unwrap() as f64 * h.confidence(self.data, i)
            )
            .sum::<f64>();


        if self.gamma_hat > edge {
            self.gamma_hat = edge;
        }


        // At this point, the stopping criterion is not satisfied.
        // Append a new hypothesis to `self.classifiers`.
        self.classifiers.push(h);

        // Update the parameters
        if self.update_params_mut().is_none() {
            self.terminated = iteration;
            return State::Terminate;
        }

        State::Continue
    }


    fn postprocess<W>(
        &mut self,
        _weak_learner: &W,
    ) -> CombinedHypothesis<F>
        where W: WeakLearner<Hypothesis = F>
    {
        // Set the weights on the hypotheses
        // by solving a linear program
        let clfs = match self.set_weights() {
            Err(e) => {
                panic!("{e}");
            },
            Ok(weights) => {
                weights.into_iter()
                    .zip(self.classifiers.clone())
                    .filter(|(w, _)| *w != 0.0)
                    .collect::<Vec<(f64, F)>>()
            }
        };

        CombinedHypothesis::from(clfs)
    }
}



impl<F> Logger for SoftBoost<'_, F>
    where F: Classifier
{
    fn weights_on_hypotheses(&mut self) {
        self.weights = self.set_weights().unwrap();
    }

    /// AdaBoost optimizes the exp loss
    fn objective_value(&self) -> f64
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
