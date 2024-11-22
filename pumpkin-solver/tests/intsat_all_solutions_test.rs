#![cfg(test)]

mod helpers;

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use helpers::check_solution;

static EXP_ID: &str = "12/0";

#[test]
pub(crate) fn check_matching_solutions() -> Result<(), Box<dyn Error>> {
    let problem_base_path = &PathBuf::from(format!(
        "{}/benches/examples-set",
        env!("CARGO_MANIFEST_DIR")
    ));

    let results_base_path = &PathBuf::from(format!(
        "{}/benches/results/experiments/{EXP_ID}",
        env!("CARGO_MANIFEST_DIR")
    ));

    for problem_set in problem_base_path.read_dir()? {
        let mut problem_set_path = &problem_base_path.join(problem_set?.path());

        for problem in problem_set_path.read_dir()? {
            let expected_path = &problem?.path();
            if expected_path.extension().unwrap() != "ozn" {
                continue;
            }

            if fs::read_to_string(expected_path).unwrap().contains("=====UNSATISFIABLE=====") {
                continue;
            }

            let problem_name = format!(
                "/{}",
                expected_path
                    .with_extension("fzn")
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            );

            // OZN found, now find the result
            for result_dir in results_base_path.read_dir()? {
                let result_path = &result_dir?.path();

                let run_info_path = &result_path.join("run_info");
                if !run_info_path.exists() || !fs::read_to_string(run_info_path)?.contains(&problem_name) {
                    continue;
                }

                // Found the result dir
                let err_path = &result_path.join("stderr");
                if !fs::read_to_string(err_path)?.is_empty() {
                    continue;
                }

                let outputs_path = &result_path.join("run_outputs");

                println!(
                    "Checking {:?} in {:?}",
                    expected_path.file_name().unwrap().to_str().unwrap(),
                    outputs_path
                );
                check_solution::<false, _>(expected_path, outputs_path);
            }
        }
    }

    Ok(())
}
