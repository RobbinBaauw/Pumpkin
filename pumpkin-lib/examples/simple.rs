use pumpkin_lib::results::{ProblemSolution, SatisfactionResult};
use pumpkin_lib::termination::Indefinite;
use pumpkin_lib::{constraints, Solver};
use std::num::NonZero;

fn main() {
    let mut solver = Solver::default();

    let x1 = solver.new_named_literal("x1");
    let x2 = solver.new_named_literal("x2");
    let x3 = solver.new_named_literal("x3");
    let x4 = solver.new_named_literal("x4");
    let x5 = solver.new_named_literal("x5");

    let literals = vec![x1, x2, x3, x4, x5];

    let _ = solver.add_constraint(constraints::clause(vec![x1, x2])).post();
    let _ = solver.add_constraint(constraints::clause(vec![!x1, x3])).post();
    let _ = solver.add_constraint(constraints::clause(vec![!x3, x4])).post();
    let _ = solver.add_constraint(constraints::clause(vec![x5, !x3])).post();
    let _ = solver.add_constraint(constraints::clause(vec![x1, !x5])).post();
    let _ = solver.add_constraint(constraints::clause(vec![x5, !x4])).post();
    let _ = solver.add_constraint(constraints::clause(vec![x4, !x1])).post();
    let _ = solver.add_constraint(constraints::clause(vec![x5, !x3])).post();
    let _ = solver.add_constraint(constraints::clause(vec![x2, !x3])).post();

    // let _ = solver.add_constraint(constraints::clause(vec![x4, x5])).post();
    let _ = solver.add_constraint(constraints::clause(vec![x4, !x5])).post();

    let _ = solver.add_constraint(constraints::clause(vec![x5, !x2])).post();
    // let _ = solver.add_constraint(constraints::clause(vec![!x5, !x2])).post();

    let mut brancher = solver.default_brancher_over_all_propositional_variables();
    match solver.satisfy(&mut brancher, &mut Indefinite) {
        SatisfactionResult::Satisfiable(solution) => {
            for (pos, literal) in literals.iter().enumerate() {
                let act_pos = pos + 1;
                let assignment = solution.get_literal_value(*literal);
                println!("x{act_pos}: {assignment}")
            }

            println!("Solved");
        }
        SatisfactionResult::Unsatisfiable => {
            println!("UNSAT");
        }
        SatisfactionResult::Unknown => {
            println!("Timeout.");
        }
    }
}
