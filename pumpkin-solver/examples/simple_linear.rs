use pumpkin_solver::{constraints, Solver};
use pumpkin_solver::results::{ProblemSolution, SatisfactionResult};
use pumpkin_solver::termination::Indefinite;
use pumpkin_solver::variables::TransformableVariable;

fn main() {
    let n = 3;

    let mut solver = Solver::default();

    let x = solver.new_named_bounded_integer(0, 10, "x");
    let y = solver.new_named_bounded_integer(0, 10, "x");
    let z = solver.new_named_bounded_integer(0, 10, "x");

    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![
        x.scaled(1),
        y.scaled(1),
        z.scaled(2)
    ], 2)).post();

    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![
        x.scaled(1),
        y.scaled(1),
        z.scaled(-2)
    ], 0)).post();

    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![
        y.scaled(-1),
    ], -2)).post();

    let mut brancher = solver.default_brancher_over_all_propositional_variables();
    match solver.satisfy(&mut brancher, &mut Indefinite) {
        SatisfactionResult::Satisfiable(solution) => {
            for variable in vec![x, y, z] {
                let val = solution.get_integer_value(variable) as u32;
                println!("{n}: {val}");
            }
        },
        SatisfactionResult::Unsatisfiable => panic!("help"),
        SatisfactionResult::Unknown => panic!("help2")
    }
}