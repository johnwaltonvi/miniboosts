use polars::prelude::*;

use std::env;

use lycaon::prelude::*;


/// Tests for `ERLPBoost`.
#[cfg(test)]
pub mod erlpboost_iris {
    use super::*;
    #[test]
    fn iris() {
        let mut path = env::current_dir().unwrap();
        println!("path: {:?}", path);
        path.push("tests/iris.csv");

        let df = CsvReader::from_path(path)
            .unwrap()
            .has_header(true)
            .finish()
            .unwrap();


        let (m, _) = df.shape();

        let m = m as f64;


        let mask = df.column("class")
            .unwrap()
            .i64()
            .unwrap()
            .not_equal(0);

        let mut df = df.filter(&mask).unwrap();
        let data = df.apply("class", |col| {
                col.i64().unwrap().into_iter()
                    .map(|v| v.map(|i| if i == 1 { -1 } else { 1 }))
                    .collect::<Int64Chunked>()
            }).unwrap();


        let target = data.drop_in_place(&"class").unwrap();


        // let nu = 0.1_f64;
        let nu = 1.0_f64 / m;
        let mut booster = ERLPBoost::init(&data)
            .capping(nu * m);

        let dtree = DTree::init(&data)
            .max_depth(1);


        let f = booster.run(&dtree, &data, &target, 0.1);


        let (m, _) = data.shape();
        let predictions = f.predict_all(&data);

        let loss = target.i64().unwrap()
            .into_iter()
            .zip(predictions)
            .map(|(t, p)| if t.unwrap() != p { 1.0 } else { 0.0 })
            .sum::<f64>() / m as f64;

        println!("Loss (iris.csv, ERLPBoost, DTree): {loss}");
        println!("classifier: {f:?}");
        assert!(true);
    }
}


