//! Provides [`GraphSepBoost`](Graph Separation Boosting)
//! by Noga Alon, Alon Gonen, Elad Hazan, and Shay Moran, 2023.
use crate::{
    Booster,
    WeakLearner,
    Classifier,
    NaiveAggregation,
    Sample,

    research::Research,
};

use std::ops::ControlFlow;
use std::collections::HashSet;


/// The Graph Separation Boosting algorithm proposed by Robert E. Schapire and Yoav Freund.
/// 
/// The algorithm is comes from the following paper: 
/// [Boosting Simple Learners](https://theoretics.episciences.org/10757/pdf)
/// by Noga Alon, Alon Gonen, Elad Hazan, and Shay Moran.
/// 
/// Given a `γ`-weak learner and a set `S` of training examples of size `m`,
/// `GraphSepBoost` terminates in `O( ln(m) / γ)` rounds.
///
/// To guarantee the generalization ability,
/// one needs to use a **simple** weak-learner.
/// 
/// # Example
/// The following code shows a small example 
/// for running [`Graph Separation Boosting`](Graph Separation Boosting).  
/// See also:
/// - [`DecisionTree`]
/// - [`DecisionTreeClassifier`]
/// - [`NaiveAggregation<F>`]
/// - [`Sample`]
/// 
/// [`DecisionTree`]: crate::weak_learner::DecisionTree
/// [`DecisionTreeClassifier`]: crate::weak_learner::DecisionTreeClassifier
/// [`NaiveAggregation<F>`]: crate::hypothesis::NaiveAggregation
/// 
/// 
/// ```no_run
/// use miniboosts::prelude::*;
/// 
/// // Read the training sample from the CSV file.
/// // We use the column named `class` as the label.
/// let sample = SampleReader::new()
///     .file(path_to_file)
///     .has_header(true)
///     .target_feature("class")
///     .read()
///     .unwrap();
/// 
/// let mut booster = GraphSepBoost::init(&sample);
/// 
/// // Set the weak learner with setting parameters.
/// let weak_learner = DecisionTreeBuilder::new(&sample)
///     .max_depth(1)
///     .criterion(Criterion::Entropy)
///     .build();
/// 
/// // Run `GraphSepBoost` and obtain the resulting hypothesis `f`.
/// let f = booster.run(&weak_learner);
/// 
/// // Get the predictions on the training set.
/// let predictions = f.predict_all(&sample);
/// 
/// // Get the number of training examples.
/// let n_sample = sample.shape().0 as f64;
/// 
/// // Calculate the training loss.
/// let target = sample.target();
/// let training_loss = target.into_iter()
///     .zip(predictions)
///     .map(|(&y, fx)| if y as i64 == fx { 0.0 } else { 1.0 })
///     .sum::<f64>()
///     / n_sample;
/// 
///
/// println!("Training Loss is: {training_loss}");
/// ```
pub struct GraphSepBoost<'a, F> {
    // Training sample
    sample: &'a Sample,


    // The number of edges of each vertex (which corresponds to some instance)
    edges: Vec<HashSet<usize>>,


    // Hypohteses obtained by the weak-learner.
    hypotheses: Vec<F>,


    // The number of edges at the end of the previous round.
    n_edges: usize,
}


impl<'a, F> GraphSepBoost<'a, F> {
    /// Constructs a new instance of `GraphSepBoost`.
    /// 
    /// Time complexity: `O(1)`.
    #[inline]
    pub fn init(sample: &'a Sample) -> Self {
        Self {
            sample,
            hypotheses: Vec::new(),
            edges: Vec::new(),
            n_edges: usize::MAX,
        }
    }
}

impl<'a, F> GraphSepBoost<'a, F>
    where F: Classifier
{
    /// Returns a weight on the new hypothesis.
    /// `update_params` also updates `self.dist`.
    /// 
    /// `GraphSepBoost` uses exponential update,
    /// which is numerically unstable so that I adopt a logarithmic computation.
    /// 
    /// Time complexity: `O( m ln(m) )`,
    /// where `m` is the number of training examples.
    /// The additional `ln(m)` term comes from the numerical stabilization.
    #[inline]
    fn update_params(&mut self, h: &F) {
        let predictions = h.predict_all(self.sample);

        let (n_sample, _) = self.sample.shape();
        for i in 0..n_sample {
            for j in i+1..n_sample {
                if predictions[i] != predictions[j] {
                    self.edges[i].remove(&j);
                    self.edges[j].remove(&i);
                }
            }
        }
    }
}


impl<F> Booster<F> for GraphSepBoost<'_, F>
    where F: Classifier + Clone,
{
    type Output = NaiveAggregation<F>;


    fn name(&self) -> &str {
        "Graph Separation Boosting"
    }


    fn info(&self) -> Option<Vec<(&str, String)>> {
        let (n_sample, n_feature) = self.sample.shape();
        let info = Vec::from([
            ("# of examples", format!("{n_sample}")),
            ("# of features", format!("{n_feature}")),
        ]);
        Some(info)
    }


    fn preprocess<W>(
        &mut self,
        _weak_learner: &W,
    )
        where W: WeakLearner<Hypothesis = F>
    {
        self.sample.is_valid_binary_instance();
        // Initialize parameters
        let n_sample = self.sample.shape().0;

        let target = self.sample.target();

        self.edges = vec![HashSet::new(); n_sample];
        for i in 0..n_sample {
            for j in i+1..n_sample {
                if target[i] != target[j] {
                    self.edges[i].insert(j);
                    self.edges[j].insert(i);
                }
            }
        }

        self.n_edges = self.edges
            .iter()
            .map(|edges| edges.len())
            .sum();

        self.hypotheses = Vec::new();
    }


    fn boost<W>(
        &mut self,
        weak_learner: &W,
        iteration: usize,
    ) -> ControlFlow<usize>
        where W: WeakLearner<Hypothesis = F>,
    {
        if self.n_edges == 0 {
            return ControlFlow::Break(iteration);
        }

        let dist = self.edges.iter()
            .map(|edge| edge.len() as f64 / self.n_edges as f64)
            .collect::<Vec<_>>();

        // Get a new hypothesis
        let h = weak_learner.produce(self.sample, &dist);
        self.update_params(&h);
        self.hypotheses.push(h);


        let n_edges = self.edges
            .iter()
            .map(|edges| edges.len())
            .sum::<usize>();
        if self.n_edges == n_edges {
            eprintln!("[WARN] number of edges does not decrease.");
            return ControlFlow::Break(iteration+1);
        }
        self.n_edges = n_edges;

        ControlFlow::Continue(())
    }


    fn postprocess<W>(
        &mut self,
        _weak_learner: &W,
    ) -> Self::Output
        where W: WeakLearner<Hypothesis = F>
    {
        let hypotheses = std::mem::take(&mut self.hypotheses);
        NaiveAggregation::new(hypotheses, &self.sample)
    }
}


impl<H> Research for GraphSepBoost<'_, H>
    where H: Classifier + Clone,
{
    type Output = NaiveAggregation<H>;
    fn current_hypothesis(&self) -> Self::Output {
        NaiveAggregation::from_slice(&self.hypotheses, &self.sample)
    }
}
