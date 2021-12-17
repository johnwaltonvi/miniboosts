pub mod core;
pub mod adaboost;
pub mod adaboostv;
pub mod lpboost;
pub mod erlpboost;
pub mod softboost;
pub mod totalboost;

// Export the Boosters
pub use self::core::Booster;
pub use adaboost::AdaBoost;
pub use lpboost::LPBoost;
pub use erlpboost::ERLPBoost;
pub use softboost::SoftBoost;
pub use totalboost::TotalBoost;
