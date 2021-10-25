use crate::data_type::{Data, Label, Sample};
use crate::booster::core::Booster;
use crate::base_learner::core::Classifier;
use crate::base_learner::core::BaseLearner;



/// Struct `AdaBoost` has 3 parameters.
/// `dist` is the distribution over training examples,
/// `weights` is the weights over `classifiers` that the AdaBoost obtained up to iteration `t`.
/// `classifiers` is the classifier that the AdaBoost obtained.
/// The length of `weights` and `classifiers` must be same.
pub struct AdaBoost<D, L> {
    pub dist: Vec<f64>,
    pub weights: Vec<f64>,
    pub classifiers: Vec<Box<dyn Classifier<D, L>>>,
}


impl<D, L> AdaBoost<D, L> {
    pub fn init(sample: &Sample<D, L>) -> AdaBoost<D, L> {
        let m = sample.len();
        assert!(m != 0);
        let uni = 1.0 / m as f64;
        AdaBoost {
            dist: vec![uni; m], weights: Vec::new(), classifiers: Vec::new()
        }
    }


    /// `max_loop` returns the maximum iteration of the Adaboost to find a combined hypothesis
    /// that has error at most `eps`.
    pub fn max_loop(&self, eps: f64) -> usize {
        let m = self.dist.len();

        ((m as f64).ln() / (eps * eps)) as usize
    }


}


impl<D> Booster<D, f64> for AdaBoost<D, f64> {

    /// `update_params` updates `self.distribution` and determine the weight on hypothesis
    /// that the algorithm obtained at current iteration.
    fn update_params(&mut self, h: Box<dyn Classifier<D, f64>>, sample: &Sample<D, f64>) -> Option<()> {


        let m = sample.len();

        let mut edge = 0.0;
        for i in 0..m {
            let data  = &sample[i].data;
            let label = &sample[i].label;
            edge += self.dist[i] * label * h.predict(data);
        }


        // This assertion may fail because of the numerical error
        // dbg!(edge);
        // assert!(edge >= -1.0);
        // assert!(edge <=  1.0);


        if edge >= 1.0 {
            self.weights.clear();
            self.classifiers.clear();

            self.weights.push(1.0);
            self.classifiers.push(h);

            return None;
        }


        let weight_of_h = ((1.0 + edge) / (1.0 - edge)).ln() / 2.0;


        // To prevent overflow, take the logarithm.
        for i in 0..m {
            let data  = &sample[i].data;
            let label = &sample[i].label;
            self.dist[i] = self.dist[i].ln() - weight_of_h * label * h.predict(&data);
        }

        let mut indices = (0..m).collect::<Vec<usize>>();
        indices.sort_unstable_by(|&i, &j| self.dist[i].partial_cmp(&self.dist[j]).unwrap());


        let mut normalizer = self.dist[indices[0]];
        for i in 1..m {
            let mut a = normalizer;
            let mut b = self.dist[indices[i]];
            if a < b {
                std::mem::swap(&mut a, &mut b);
            }

            normalizer = a + (1.0 + (b - a).exp()).ln();
        }

        for i in 0..m {
            self.dist[i] = (self.dist[i] - normalizer).exp();
        }

        self.classifiers.push(h);
        self.weights.push(weight_of_h);

        Some(())
    }


    fn run(&mut self, base_learner: Box<dyn BaseLearner<D, f64>>, sample: &Sample<D, f64>, eps: f64) {
        let max_loop = self.max_loop(eps);
        dbg!(format!("max_loop: {}", max_loop));
    
        for _t in 1..max_loop {
            let h = base_learner.best_hypothesis(sample, &self.dist);
            if let None = self.update_params(h, sample) {
                println!("Break loop at: {}", _t);
                break;
            }
        }
    }


    fn predict(&self, data: &Data<D>) -> Label<f64> {
        assert_eq!(self.weights.len(), self.classifiers.len());
        let n = self.weights.len();

        let mut confidence = 0.0;
        for i in 0..n {
            confidence += self.weights[i] * self.classifiers[i].predict(data);
        }


        confidence.signum()
    }
}
