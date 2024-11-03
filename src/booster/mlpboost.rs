//! MLPBoost module.

pub mod mlpboost_algorithm;

#[cfg(not(feature="gurobi"))]
mod lp_model;

#[cfg(feature="gurobi")]
mod gurobi_lp_model;


pub use mlpboost_algorithm::MLPBoost;
