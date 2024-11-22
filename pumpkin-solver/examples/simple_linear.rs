use itertools::Itertools;
use pumpkin_solver::constraints;
use pumpkin_solver::results::ProblemSolution;
use pumpkin_solver::results::SatisfactionResult;
use pumpkin_solver::statistics::configure_statistic_logging;
use pumpkin_solver::termination::Indefinite;
use pumpkin_solver::variables::DomainId;
use pumpkin_solver::variables::TransformableVariable;
use pumpkin_solver::Solver;

fn super_simple_tests(solver: &mut Solver) -> Vec<DomainId> {
    let x = solver.new_named_bounded_integer(0, 3, "x");
    let y = solver.new_named_bounded_integer(0, 3, "y");

    let _ = solver
        .add_constraint(constraints::less_than_or_equals(vec![x * 3, y * 4], -2))
        .post();

    let _ = solver
        .add_constraint(constraints::less_than_or_equals(vec![-y], -1))
        .post();

    vec![x, y]
}

fn intsat_paper_example(solver: &mut Solver) -> Vec<DomainId> {
    let x = solver.new_named_bounded_integer(-10, 1, "x");
    let y = solver.new_named_bounded_integer(-10, 1, "y");
    let z = solver.new_named_bounded_integer(-10, 3, "z");

    let _ = solver
        .add_constraint(constraints::less_than_or_equals(
            vec![x.scaled(-1), y.scaled(-1), z.scaled(-1)],
            -2,
        ))
        .post();

    let _ = solver
        .add_constraint(constraints::less_than_or_equals(
            vec![x.scaled(1), y.scaled(1)],
            1,
        ))
        .post();

    let _ = solver
        .add_constraint(constraints::less_than_or_equals(
            vec![x.scaled(1), z.scaled(1)],
            1,
        ))
        .post();

    let _ = solver
        .add_constraint(constraints::less_than_or_equals(
            vec![y.scaled(1), z.scaled(1)],
            1,
        ))
        .post();

    vec![x, y, z]
}

fn nqueens_ilp(solver: &mut Solver) -> Vec<DomainId> {
    let n = 20;

    let variables = (0..n)
        .map(|row| {
            (0..n)
                .map(|col| solver.new_named_bounded_integer(0, 1, format!("q({row}, {col})")))
                .collect_vec()
        })
        .collect_vec();

    for row_i in 0..n {
        // Check horizontal uniqueness
        let vars_in_row = variables[row_i].clone();
        let _ = solver
            .add_constraint(constraints::equals(vars_in_row, 1))
            .post();

        // Diag left to right
        let vars_in_diag_lr = (0..n - row_i)
            .map(|k| variables[row_i + k][k])
            .collect_vec();
        let _ = solver
            .add_constraint(constraints::less_than_or_equals(vars_in_diag_lr, 1))
            .post();

        // Diag right to left
        let vars_in_diag_rl = (0..n - row_i)
            .map(|k| variables[row_i + k][n - k - 1])
            .collect_vec();
        let _ = solver
            .add_constraint(constraints::less_than_or_equals(vars_in_diag_rl, 1))
            .post();
    }

    for col_i in 0..n {
        // Check vertical uniqueness
        let vars_in_col = (0..n).map(|row| variables[row][col_i]).collect_vec();
        let _ = solver
            .add_constraint(constraints::equals(vars_in_col, 1))
            .post();

        // Diag left to right
        let vars_in_diag_lr = (0..n - col_i)
            .map(|k| variables[k][col_i + k])
            .collect_vec();
        let _ = solver
            .add_constraint(constraints::less_than_or_equals(vars_in_diag_lr, 1))
            .post();

        // Diag right to left
        let vars_in_diag_rl = (0..n - col_i)
            .map(|k| variables[k][n - col_i - k - 1])
            .collect_vec();
        let _ = solver
            .add_constraint(constraints::less_than_or_equals(vars_in_diag_rl, 1))
            .post();
    }

    variables.into_iter().flatten().collect_vec()
}

fn pigeonhole(solver: &mut Solver) -> Vec<DomainId> {
    let pigeons = 100;
    let holes = 80;

    let holes_vars = (0..holes)
        .map(|h| solver.new_named_bounded_integer(0, pigeons, format!("h{h}")))
        .collect_vec();

    // We need to put all pigeons in holes
    let _ = solver
        .add_constraint(constraints::equals(holes_vars.clone(), pigeons))
        .post();

    // Each hole can take at most 1 pigeon
    (0..holes).for_each(|h| {
        let _ = solver
            .add_constraint(constraints::less_than_or_equals(vec![holes_vars[h]], 1))
            .post();
    });

    holes_vars
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .format_target(false)
        .format_timestamp(None)
        .init();

    let mut solver = Solver::default();

    configure_statistic_logging("stat", None, None, None);

    // let vars = super_simple_tests(&mut solver);
    // let vars = intsat_paper_example(&mut solver);
    // let vars = next_test(&mut solver);
    let vars = nqueens_ilp(&mut solver);
    // let vars = pigeonhole(&mut solver);

    let mut brancher = solver.default_brancher();

    match solver.satisfy(&mut brancher, &mut Indefinite) {
        SatisfactionResult::Satisfiable(solution) => {
            for variable in &vars {
                let val = solution.get_integer_value(*variable) as u32;
                println!("{variable}: {val}");
            }

            // let n = (vars.len() as f64).sqrt() as usize;
            //
            // let row_separator = format!("{}+", "+---".repeat(n as usize));
            //
            // for row in 0..n {
            //     println!("{row_separator}");
            //
            //     for col in 0..n {
            //         let queen_col = solution.get_integer_value(vars[row * n + col]) as u32;
            //         let string = if queen_col == 1 { "| * " } else { "|   " };
            //
            //         print!("{string}");
            //     }
            //
            //     println!("|");
            // }
            //
            // println!("{row_separator}");
        }
        SatisfactionResult::Unsatisfiable => panic!("unsat"),
        SatisfactionResult::Unknown => panic!("unknown"),
    }

    solver.log_statistics();
}
