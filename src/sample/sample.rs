use std::path::Path;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::collections::HashMap;
use std::ops::Index;
use std::cell::{Ref, RefMut, RefCell};
use std::mem;

use polars::prelude::*;
use rayon::prelude::*;
use super::feature::*;


/// Struct `Sample` holds a batch sample with dense/sparse format.
#[derive(Debug)]
pub struct Sample {
    pub(super) name_to_index: HashMap<String, usize>,
    pub(super) features: Vec<Feature>,
    pub(super) target: RefCell<Vec<f64>>,
    pub(super) n_sample: usize,
    pub(super) n_feature: usize,
}


impl Sample {
    /// Convert `polars::DataFrame` and `polars::Series` into
    /// `Sample`.
    /// This method takes the ownership for the given pair
    /// `data` and `target`.
    pub fn from_dataframe(data: DataFrame, target: Series)
        -> io::Result<Self>
    {
        let (n_sample, n_feature) = data.shape();
        let target = target.f64()
            .expect("The target is not a dtype f64")
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .unwrap();
        let target = RefCell::new(target);

        let features = data.get_columns()
            .into_par_iter()
            .map(|series| 
                Feature::Dense(DenseFeature::from_series(series))
            )
            .collect::<Vec<_>>();

        let name_to_index = features.iter()
            .enumerate()
            .map(|(i, f)| (f.name().to_string(), i))
            .collect::<HashMap<_, _>>();

        let sample = Self {
            name_to_index, features, target, n_sample, n_feature,
        };
        Ok(sample)
    }


    /// Read a CSV format file to `Sample` type.
    pub fn from_csv<P>(file: P, mut has_header: bool) -> io::Result<Self>
        where P: AsRef<Path>,
    {
        // Open the given `file`.
        let file = File::open(file)?;
        let mut lines = BufReader::new(file).lines();

        let mut features = Vec::new();
        if has_header {
            let line = lines.next().unwrap();
            features = line?.split(',')
                .map(|name| DenseFeature::new(name))
                .collect::<Vec<_>>();
        }
        let mut n_sample = 0_usize;

        // For each line of the file
        for line in lines {
            // Split the line by white spaces
            let line = line?;

            // if the headeer does not exists,
            // construct a dummy header.
            if !has_header {
                let xs = line.split(',')
                    .map(|x| x.trim().parse::<f64>().unwrap())
                    .collect::<Vec<_>>();

                let n_feature = xs.len();
                features = (1..=n_feature).into_iter()
                    .map(|i| {
                        let name = format!("Feat. [{i}]");
                        DenseFeature::new(name)
                    })
                    .collect::<Vec<_>>();

                for (feat, x) in features.iter_mut().zip(xs) {
                    feat.append(x);
                }

                has_header = true;
                n_sample += 1;
                continue;
            }

            line.split(',')
                .map(|x| x.trim().parse::<f64>().unwrap())
                .enumerate()
                .for_each(|(i, x)| {
                    features[i].append(x);
                });

            n_sample += 1;
        }

        let features = features.into_par_iter()
            .map(|feat| Feature::Dense(feat))
            .collect::<Vec<_>>();

        let n_feature = features.len();
        let target = Vec::with_capacity(0);
        let target = RefCell::new(target);

        let name_to_index = features.iter()
            .enumerate()
            .map(|(i, f)| (f.name().to_string(), i))
            .collect::<HashMap<_, _>>();

        let sample = Self {
            name_to_index, features, target, n_sample, n_feature,
        };

        Ok(sample)
    }


    /// Returns a slice of type `f64`.
    pub fn target(&self) -> Ref<'_, [f64]> {
    // pub fn target(&self) -> &[f64] {
        // &self.target[..]
        // &self.target.borrow()[..]
        Ref::map(self.target.borrow(), |x| &x[..])
    }


    /// Returns a mutable slice of type `f64`.
    /// This method is required to modify the target values.
    pub fn target_mut(&self) -> RefMut<'_, [f64]> {
        RefMut::map(self.target.borrow_mut(), |x| &mut x[..])
    }


    /// Returns a slice of type `Feature`.
    pub fn features(&self) -> &[Feature] {
        &self.features[..]
    }


    /// Set the feature of name `target` to `self.target`.
    /// The old value assigned to `self.target` will be dropped.
    pub fn set_target<S: AsRef<str>>(mut self, target: S) -> Self {
        let target = target.as_ref();
        let pos = self.features.iter()
            .position(|feat| feat.name() == target)
            .expect("The target class does not exist");


        let target = self.features.remove(pos).into_target();
        self.target = RefCell::new(target);
        self.n_feature -= 1;


        self.name_to_index = self.features.iter()
            .enumerate()
            .map(|(i, f)| (f.name().to_string(), i))
            .collect::<HashMap<_, _>>();

        self
    }


    /// Read a SVMLight format file to `Sample` type.
    /// 
    /// Each line of SVMLight format file has the following form:
    /// ```txt
    /// y index:value index: value
    /// ```
    /// where `y` is the target label of type `f64`,
    /// `index` is the feature index, and `value` is the value
    /// at the feature.
    /// 
    /// **Note**
    /// The SVMLight format file is basically 1-indexed,
    /// while the `sklearn.datasets.dump_svmlight_file` outputs
    /// a svmlight format file with 0-indexed, by default.
    pub fn from_svmlight<P: AsRef<Path>>(file: P) -> io::Result<Self> {
        let mut features = Vec::new();
        let mut target = Vec::new();
        let mut n_sample = 0_usize;

        // Open the given `file`.
        let file = File::open(file)?;
        let lines = BufReader::new(file).lines();

        // For each line of the file
        for line in lines {
            // Split the line by white spaces
            let line = line?;
            let mut words = line.split_whitespace();
            // The first word corresponds to the target value.
            let y = words.next().unwrap().trim().parse::<f64>().unwrap();
            target.push(y);

            for word in words {
                let (i, x) = index_and_feature(word);

                while features.len() <= i {
                    let k = features.len() + 1;
                    let name = format!("Feat. [{k}]");
                    features.push(SparseFeature::new(name));
                }

                features[i].append((n_sample, x));
            }
            n_sample += 1;
        }

        let target = RefCell::new(target);
        let n_feature = features.len();


        let features = features.into_iter()
            .map(|mut feat| {
                feat.n_sample = n_sample;
                Feature::Sparse(feat)
            })
            .collect::<Vec<_>>();

        let name_to_index = features.iter()
            .enumerate()
            .map(|(i, f)| (f.name().to_string(), i))
            .collect::<HashMap<_, _>>();

        let mut sample = Self {
            name_to_index, features, target, n_sample, n_feature,
        };

        sample.remove_allzero_features();

        Ok(sample)
    }


    /// Removes the empty features in `self.features`.
    fn remove_allzero_features(&mut self) {
        let features = mem::replace(&mut self.features, vec![]);
        self.name_to_index = features.iter()
            .filter_map(|feat| {
                if feat.len() > 0 {
                    Some(feat.name().to_string())
                } else {
                    None
                }
            })
            .enumerate()
            .map(|(i, name)| (name, i))
            .collect();
        self.features = features.into_iter()
            .filter(|feat| feat.len() > 0)
            .collect();
        self.n_feature = self.features.len();
    }


    /// Returns the pair of the number of examples and
    /// the number of features
    pub fn shape(&self) -> (usize, usize) {
        (self.n_sample, self.n_feature)
    }


    /// Set the feature (column) names.
    /// This method panics when the length of given feature names is
    /// not equals to the one of `self.features`.
    pub fn replace_names<S, T>(&mut self, names: T) -> Vec<String>
        where S: ToString + std::fmt::Display,
              T: AsRef<[S]>,
    {
        let names = names.as_ref();

        let n_features = self.shape().1;
        let n_names = names.len();
        if n_features != n_names {
            panic!("The number of names is not equals to the one of `self.features`");
        }

        let old_names = names.into_iter()
            .zip(&mut self.features[..])
            .map(|(name, feature)| feature.replace_name(name))
            .collect();

        self.name_to_index = self.features.iter()
            .map(|feature| feature.name().to_string())
            .enumerate()
            .map(|(i, name)| (name, i))
            .collect();
        old_names
    }


    /// Returns the `idx`-th instance `(x, y)`.
    pub fn at(&self, idx: usize) -> (Vec<f64>, f64) {
        let x = self.features.iter()
            .map(|feat| feat[idx])
            .collect::<Vec<f64>>();
        let y = self.target.borrow()[idx];

        (x, y)
    }
}


/// Parse the following type of `str` to the pair of `(usize, f64)`.
/// `index:value`, where `index: usize` and `value: f64`.
pub(self) fn index_and_feature(word: &str) -> (usize, f64) {
    let mut i_x = word.split(':');
    let i = i_x.next().unwrap().trim().parse::<usize>().unwrap();
    let x = i_x.next().unwrap().trim().parse::<f64>().unwrap();

    (i, x)
}



impl<S> Index<S> for Sample
    where S: AsRef<str>
{
    type Output = Feature;


    fn index(&self, name: S) -> &Self::Output {
        let name: &str = name.as_ref();
        let k = *self.name_to_index.get(name).unwrap();
        &self.features[k]
    }
}
