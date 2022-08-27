//! This file defines `ERLPBoost` based on the paper
//! "Entropy Regularized LPBoost"
//! by Warmuth et al.
//! 
use polars::prelude::*;
// use rayon::prelude::*;


use crate::{Classifier, CombinedClassifier};
use crate::BaseLearner;
use crate::Booster;
use grb::prelude::*;



/// ERLPBoost struct. 
/// See [this paper](https://citeseerx.ist.psu.edu/viewdoc/download?doi=10.1.1.141.1759&rep=rep1&type=pdf).
pub struct ERLPBoost {
    dist: Vec<f64>,

    // `gamma_hat` corresponds to $\min_{q=1, .., t} P^q (d^{q-1})$
    gamma_hat: f64,

    // `gamma_star` corresponds to $P^{t-1} (d^{t-1})$
    gamma_star: f64,
    // regularization parameter defined in the paper
    eta: f64,

    tolerance: f64,


    // an accuracy parameter for the sub-problems
    sub_tolerance: f64,
    capping_param: f64,
    env: Env,


    terminated: usize,
}


impl ERLPBoost {
    /// Initialize the `ERLPBoost`.
    /// Use `data` for argument.
    /// This method does not care 
    /// whether the label is included in `data` or not.
    pub fn init(df: &DataFrame) -> Self {
        let (m, _) = df.shape();
        assert!(m != 0);

        let mut env = Env::new("").unwrap();

        env.set(param::OutputFlag, 0).unwrap();

        // Set uni as an uniform weight
        let uni = 1.0 / m as f64;

        // Compute $\ln(m)$ in advance
        let ln_m = (m as f64).ln();


        // Set tolerance
        let tolerance = uni / 2.0;


        // Set regularization parameter
        let eta = 0.5_f64.max(2.0_f64 * ln_m / tolerance);

        // Set gamma_hat and gamma_star
        let gamma_hat  = 1.0 + (ln_m / eta);
        let gamma_star = f64::MIN;


        ERLPBoost {
            dist:          vec![uni; m],
            gamma_hat,
            gamma_star,
            eta,
            tolerance,
            sub_tolerance: 1e-9,
            capping_param: 1.0,
            env,

            terminated: 0_usize,
        }
    }


    /// Updates the capping parameter.
    pub fn capping(mut self, capping_param: f64) -> Self {
        assert!(
            1.0 <= capping_param
            && capping_param <= self.dist.len() as f64
        );
        self.capping_param = capping_param;
        self.regularization_param();

        self
    }


    /// Returns the break iteration.
    /// This method returns `0` before the `.run()` call.
    #[inline(always)]
    pub fn terminated(&self) -> usize {
        self.terminated
    }


    /// Setter method of `self.tolerance`
    #[inline(always)]
    fn set_tolerance(&mut self, tolerance: f64) {
        self.tolerance = tolerance / 2.0;
    }


    /// Setter method of `self.eta`
    #[inline(always)]
    fn regularization_param(&mut self) {
        let ln_m = (self.dist.len() as f64 / self.capping_param).ln();
        let temp = ln_m / self.tolerance;


        self.eta = 0.5_f64.max(temp);
    }



    /// Set `gamma_hat` and `gamma_star`.
    #[inline]
    fn set_gamma(&mut self) {
        self.gamma_hat = 1.0;
        self.gamma_star = -1.0;
    }


    /// Set all parameters in ERLPBoost.
    #[inline]
    fn init_params(&mut self, tolerance: f64) {
        self.set_tolerance(tolerance);

        self.regularization_param();

        self.set_gamma();
    }


    /// `max_loop` returns the maximum iteration
    /// of the Adaboost to find a combined hypothesis
    /// that has error at most `tolerance`.
    fn max_loop(&mut self) -> u64 {
        let m = self.dist.len() as f64;

        let mut max_iter = 4.0 / self.tolerance;


        let ln_m = (m / self.capping_param).ln();
        let temp = 8.0 * ln_m / self.tolerance.powi(2);


        max_iter = max_iter.max(temp);

        max_iter.ceil() as u64
    }
}


impl ERLPBoost {
    /// Update `self.gamma_hat`.
    /// `self.gamma_hat` holds the minimum value of the objective value.
    #[inline]
    fn update_gamma_hat_mut<C>(&mut self,
                               h: &C,
                               data: &DataFrame,
                               target: &Series)
        where C: Classifier,
    {
        let edge = target.i64()
            .expect("The target class is not a dtype i64")
            .into_iter()
            .zip(self.dist.iter().copied())
            .enumerate()
            .map(|(i, (y, d))| d * y.unwrap() as f64 * h.confidence(data, i))
            .sum::<f64>();


        let m = self.dist.len() as f64;
        let entropy = self.dist.iter()
            .copied()
            .map(|d| if d == 0.0 { 0.0 } else { d * d.ln() })
            .sum::<f64>() + m.ln();


        let obj_val = edge + (entropy / self.eta);

        self.gamma_hat = self.gamma_hat.min(obj_val);
    }


    /// Update `self.gamma_star`.
    /// `self.gamma_star` holds the current optimal value.
    fn update_gamma_star_mut<C>(&mut self,
                                classifiers: &[C],
                                data: &DataFrame,
                                target: &Series)
        where C: Classifier,
    {
        let max_edge = classifiers.iter()
            .map(|h|
                target.i64()
                    .expect("The target class is not a dtype i64")
                    .into_iter()
                    .zip(self.dist.iter().copied())
                    .enumerate()
                    .map(|(i, (y, d))|
                        d * y.unwrap() as f64 * h.confidence(data, i)
                    )
                    .sum::<f64>()
            )
            .reduce(f64::max)
            .unwrap();


        let entropy = self.dist.iter()
            .copied()
            .map(|d| d * d.ln())
            .sum::<f64>();


        let m = self.dist.len() as f64;
        self.gamma_star = max_edge + (entropy + m.ln()) / self.eta;
    }


    /// Compute the weight on hypotheses
    fn set_weights<C>(&mut self,
                      data: &DataFrame,
                      target: &Series,
                      classifiers: &[C])
        -> std::result::Result<Vec<f64>, grb::Error>
        where C: Classifier,
    {
        let mut model = Model::with_env("", &self.env)?;

        let m = self.dist.len();
        let t = classifiers.len();

        // Initialize GRBVars
        let wt_vec = (0..t).map(|j| {
                let name = format!("w[{j}]");
                add_ctsvar!(model, name: &name, bounds: 0.0..).unwrap()
            }).collect::<Vec<_>>();
        let xi_vec = (0..m).map(|i| {
                let name = format!("xi[{i}]");
                add_ctsvar!(model, name: &name, bounds: 0.0..).unwrap()
            }).collect::<Vec<_>>();
        let rho = add_ctsvar!(model, name: "rho", bounds: ..)?;


        // Set constraints
        let iter = target.i64()
            .expect("The target class is not a dtype i64")
            .into_iter()
            .zip(xi_vec.iter())
            .enumerate();

        for (i, (y, &xi)) in iter {
            let y = y.unwrap() as f64;
            let expr = wt_vec.iter()
                .zip(classifiers)
                .map(|(&w, h)| w * h.confidence(data, i))
                .grb_sum();
            let name = format!("S[{i}]");
            model.add_constr(&name, c!(y * expr >= rho - xi))?;
        }

        model.add_constr(
            "sum_is_1", c!(wt_vec.iter().grb_sum() == 1.0)
        )?;
        model.update()?;


        // Set the objective function
        let param = 1.0 / self.capping_param;
        let objective = rho - param * xi_vec.iter().grb_sum();
        model.set_objective(objective, Maximize)?;
        model.update()?;


        model.optimize()?;


        let status = model.status()?;

        if status != Status::Optimal {
            panic!("Cannot solve the primal problem. Status: {status:?}");
        }


        // Assign weights over the hypotheses

        let weights = model.get_obj_attr_batch(attr::X, wt_vec).unwrap();
        Ok(weights)
    }


    /// Updates `self.dist`
    /// This method repeatedly minimizes the quadratic approximation of 
    /// ERLPB. objective around current distribution `self.dist`.
    /// Then update `self.dist` as the optimal solution of 
    /// the approximate problem. 
    /// This method continues minimizing the quadratic objective 
    /// while the decrease of the optimal value is 
    /// greater than `self.sub_tolerance`.
    fn update_distribution<C>(&mut self,
                              classifiers: &[C],
                              data: &DataFrame,
                              target: &Series)
        where C: Classifier,
    {
        let m = self.dist.len();
        let mut old_objval: f64 = 1e6;
        loop {
            assert!(
                self.dist.iter()
                    .all(|&d| 0.0 <= d && d <= 1.0 / self.capping_param)
            );


            // Initialize GRBModel
            let mut model = Model::with_env("", &self.env).unwrap();
            let gamma = add_ctsvar!(model, name: "gamma", bounds: ..)
                .unwrap();


            // Set variables that are used in the optimization problem
            let upper_bound = 1.0 / self.capping_param;


            let grb_dist = (0..m).into_iter()
                .map(|i| {
                    let name = format!("dist[{i}]");
                    add_ctsvar!(model, name: &name, bounds: 0.0..upper_bound)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            model.update().unwrap();


            // Set constraints
            classifiers.iter()
                .enumerate()
                .for_each(|(j, h)| {
                    let iter = target.i64()
                        .expect("The target class is not a dtype i64");
                    let expr = grb_dist.iter().copied()
                        .zip(iter)
                        .enumerate()
                        .map(|(i, (d, y))| {
                            let yh = (y.unwrap() * h.predict(data, i)) as f64;
                            d * yh
                        })
                        .grb_sum();

                    let name = format!("h[{j}]");
                    model.add_constr(&name, c!(expr <= gamma)).unwrap();
                });


            model.add_constr(
                "sum_is_1", c!(grb_dist.iter().grb_sum() == 1.0_f64)
            ).unwrap();
            model.update().unwrap();


            // Set objective function
            let regularizer = self.dist.iter()
                .copied()
                .zip(grb_dist.iter())
                .map(|(d, &grb_d)| {
                    let linear = d.ln() * grb_d;
                    let quad   = (0.5_f64 / d) * (grb_d * grb_d);

                    linear + quad
                })
                .grb_sum();

            let objective = gamma + ((1.0_f64 / self.eta) * regularizer);
            model.set_objective(objective, Minimize).unwrap();
            model.update().unwrap();


            model.optimize().unwrap();


            // Check the status.
            // Panic occurs when the status is not `Status::Optimal`.
            // This will never happen
            // since the domain is a bounded & closed convex set,
            let status = model.status().unwrap();
            if status != Status::Optimal && status != Status::SubOptimal {
                panic!("Status ({status:?}) is not optimal.");
            }


            // At this point, there exists an optimal solution in `vars`
            // Check the stopping criterion 
            let objval = model.get_attr(attr::ObjVal).unwrap();


            self.dist.iter_mut()
                .zip(grb_dist)
                .for_each(|(d, grb_d)| {
                    let g = model.get_obj_attr(attr::X, &grb_d).unwrap();
                    *d = g;
                });


            let any_zero = self.dist.iter().any(|&d| 0.0 == d);
            if any_zero || old_objval - objval < self.sub_tolerance {
                break;
            }

            old_objval = objval;
        }
    }
}


impl<C> Booster<C> for ERLPBoost
    where C: Classifier
{
    fn run<B>(&mut self,
              base_learner: &B,
              data: &DataFrame,
              target: &Series,
              tolerance: f64)
        -> CombinedClassifier<C>
        where B: BaseLearner<Clf = C>,
    {
        // Initialize all parameters
        self.init_params(tolerance);


        // Get max iteration.
        let max_iter = self.max_loop();

        self.terminated = max_iter as usize;


        // This vector holds the classifiers
        // obtained from the `base_learner`.
        let mut classifiers = Vec::new();

        for step in 1..=max_iter {
            // Receive a hypothesis from the base learner
            let h = base_learner.produce(data, target, &self.dist[..]);


            // update `self.gamma_hat`
            self.update_gamma_hat_mut(&h, data, target);


            // Check the stopping criterion
            let diff = self.gamma_hat - self.gamma_star;
            if diff <= self.tolerance {
                println!("Break loop at: {step}");
                self.terminated = step as usize;
                break;
            }


            // At this point, the stopping criterion is not satisfied.
            // Append a new hypothesis to `clfs`.
            classifiers.push(h);


            // Update the parameters
            self.update_distribution(&classifiers, data, target);


            // update `self.gamma_star`.
            self.update_gamma_star_mut(&classifiers, data, target);
        }

        // Set the weights on the hypotheses
        // by solving a linear program
        let clfs = match self.set_weights(data, target, &classifiers) {
            Err(e) => {
                panic!("{e}");
            },
            Ok(weights) => {
                weights.into_iter()
                    .zip(classifiers.into_iter())
                    .filter(|(w, _)| *w != 0.0)
                    .collect::<Vec<_>>()
            }
        };

        CombinedClassifier::from(clfs)
    }
}


