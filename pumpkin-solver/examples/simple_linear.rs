use pumpkin_solver::{constraints, Solver};
use pumpkin_solver::results::{OptimisationResult, ProblemSolution, SatisfactionResult};
use pumpkin_solver::termination::Indefinite;
use pumpkin_solver::variables::TransformableVariable;

fn main() {
    let mut solver = Solver::default();

    let x = solver.new_named_bounded_integer(-10, 1, "x");
    let y = solver.new_named_bounded_integer(-10, 1, "y");
    let z = solver.new_named_bounded_integer(-10, 3, "z");

    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![
        x.scaled(-1),
        y.scaled(-1),
        z.scaled(-1),
    ], -2)).post();

    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![
        x.scaled(1),
        y.scaled(1),
    ], 1)).post();

    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![
        x.scaled(1),
        z.scaled(1),
    ], 1)).post();

    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![
        y.scaled(1),
        z.scaled(1),
    ], 1)).post();

    let mut brancher = solver.default_brancher_over_all_propositional_variables();
    match solver.satisfy(&mut brancher, &mut Indefinite) {
        SatisfactionResult::Satisfiable(solution) => {
            for variable in vec![x, y, z] {
                let val = solution.get_integer_value(variable) as u32;
                println!("{variable}: {val}");
            }
        },
        SatisfactionResult::Unsatisfiable => panic!("unsat"),
        SatisfactionResult::Unknown => panic!("unknown")
    }
}