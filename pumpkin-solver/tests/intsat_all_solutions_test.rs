#![cfg(test)]

mod helpers;

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use helpers::check_solution;

static EXP_ID: &str = "13/0";

fn find_result_corresponding_to_problem(problem_path: &PathBuf) -> Result<PathBuf, Box<dyn Error>> {
    let results_base_path = &PathBuf::from(format!(
        "{}/benches/results/experiments/{EXP_ID}",
        env!("CARGO_MANIFEST_DIR")
    ));

    let problem_name = format!("/{}", problem_path.file_name().unwrap().to_str().unwrap());

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

        let metrics_path = &result_path.join("metrics");
        if fs::read_to_string(metrics_path)?.contains("secs = 3600") {
            println!("Skipping timedout problem {problem_name}");
            continue;
        }

        return Ok(result_path.join("run_outputs"));
    }

    Err(Box::from("no such path found"))
}

#[test]
pub(crate) fn check_matching_solutions() -> Result<(), Box<dyn Error>> {
    let problem_base_path = &PathBuf::from(format!(
        "{}/benches/examples-set",
        env!("CARGO_MANIFEST_DIR")
    ));

    for problem_set in problem_base_path.read_dir()? {
        let problem_set_path = &problem_base_path.join(problem_set?.path());

        for problem in problem_set_path.read_dir()? {
            let problem_path = &problem?.path();
            if problem_path.extension().unwrap() != "fzn" {
                continue;
            }

            if !fs::read_to_string(problem_path)
                .unwrap()
                .contains("solve  satisfy")
            {
                println!(
                    "Skipping optimisation problem {}",
                    problem_path.to_str().unwrap()
                );
                continue;
            }

            let expected_path = &problem_path.with_extension("ozn");
            if !expected_path.exists() {
                println!(
                    "Missing minizinc solution {}",
                    expected_path.to_str().unwrap()
                );
                continue;
            }

            if fs::read_to_string(expected_path)?.contains("=====UNSATISFIABLE=====") {
                println!("Skipping UNSAT problem {}", problem_path.to_str().unwrap());
                continue;
            }

            if !fs::read_to_string(expected_path)?
                .trim()
                .ends_with("==========")
            {
                println!(
                    "Skipping invalid solution {}",
                    expected_path.to_str().unwrap()
                );
                continue;
            }

            let Ok(outputs_path) = find_result_corresponding_to_problem(problem_path) else {
                println!(
                    "Could not find output for {}",
                    problem_path.to_str().unwrap()
                );
                continue;
            };

            println!(
                "Checking {:?} in {:?}",
                expected_path.file_name().unwrap().to_str().unwrap(),
                outputs_path
            );

            check_solution::<false, _>(expected_path, &outputs_path);
        }
    }

    Ok(())
}
