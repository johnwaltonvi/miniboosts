//! Exports the standard boosting algorithms and traits.
//! 
pub use crate::booster::{
    // Booster trait
    Booster,


    // Classification ---------------------------
    // ERM boostings
    AdaBoost,


    // Hard margin maximizing boostings
    AdaBoostV,

    // Soft margin optimization
    SmoothBoost,
    CERLPBoost,

    // Regression
    GBM,
};

#[cfg(feature="extended")]
pub use crate::booster::{
    TotalBoost,

    // Soft margin maximizing boostings
    LPBoost,
    ERLPBoost,
    SoftBoost,

    MLPBoost,
};


pub use crate::weak_learner::{
    // Base Learner trait
    WeakLearner,


    // Classification ---------------------------
    DTree,
    DTreeClassifier,
    Criterion,


    // GaussianNB,
    // NBayesClassifier,
    NeuralNetwork,
    NNHypothesis,
    Activation,
    NNLoss,


    // Regression -------------------------------
    RTree,
    RTreeRegressor,
    LossType,
};


pub use crate::hypothesis::{
    Classifier,
    Regressor,
    CombinedHypothesis,
};

pub use crate::Sample;

pub use crate::common::{
    loss_functions::GBMLoss,
    frank_wolfe::FWType,
};

