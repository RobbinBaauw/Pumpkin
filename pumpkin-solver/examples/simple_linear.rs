use pumpkin_solver::{constraints, Solver};
use pumpkin_solver::results::{ProblemSolution, SatisfactionResult};
use pumpkin_solver::termination::Indefinite;
use pumpkin_solver::variables::{DomainId, TransformableVariable};

fn intsat_paper_example(solver: &mut Solver) -> Vec<DomainId> {
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

    vec![x, y, z]
}

fn next_test(solver: &mut Solver) -> Vec<DomainId> {
    let zz01 = solver.new_named_bounded_integer(0, 1, "zz01");
    let zz02 = solver.new_named_bounded_integer(0, 1, "zz02");
    let zz03 = solver.new_named_bounded_integer(0, 1, "zz03");
    let zz04 = solver.new_named_bounded_integer(0, 1, "zz04");
    let zz05 = solver.new_named_bounded_integer(0, 1, "zz05");
    let zz06 = solver.new_named_bounded_integer(0, 1, "zz06");
    let zz07 = solver.new_named_bounded_integer(0, 1, "zz07");
    let zz08 = solver.new_named_bounded_integer(0, 1, "zz08");
    let zz09 = solver.new_named_bounded_integer(0, 1, "zz09");
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz02.scaled(-1), zz03.scaled(-1), zz04.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz01.scaled(-1), zz03.scaled(-1), zz05.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz01.scaled(-1), zz02.scaled(-1), zz06.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz05.scaled(-1), zz06.scaled(-1), zz07.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz04.scaled(-1), zz06.scaled(-1), zz08.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz04.scaled(-1), zz05.scaled(-1), zz09.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz01.scaled(-1), zz08.scaled(-1), zz09.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz02.scaled(-1), zz07.scaled(-1), zz09.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz03.scaled(-1), zz07.scaled(-1), zz08.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz01.scaled(-1), zz04.scaled(-1), zz07.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz02.scaled(-1), zz05.scaled(-1), zz08.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz03.scaled(-1), zz06.scaled(-1), zz09.scaled(-1)], -1)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz01.scaled(-1), zz02.scaled(-1), zz03.scaled(-1), zz04.scaled(-1), zz05.scaled(-1), zz06.scaled(-1), zz07.scaled(-1), zz08.scaled(-1), zz09.scaled(-1)], -4)).post();
    let _ = solver.add_constraint(constraints::less_than_or_equals(vec![zz01.scaled(1), zz02.scaled(1), zz03.scaled(1), zz04.scaled(1), zz05.scaled(1), zz06.scaled(1), zz07.scaled(1), zz08.scaled(1), zz09.scaled(1)], 4)).post();
    vec![zz01, zz02, zz03, zz04, zz05, zz06, zz07, zz08, zz09]
}

fn main() {
    let mut solver = Solver::default();

    // let vars = intsat_paper_example(&mut solver);
    let vars = next_test(&mut solver);

    let mut brancher = solver.default_brancher_over_all_propositional_variables();
    match solver.satisfy(&mut brancher, &mut Indefinite) {
        SatisfactionResult::Satisfiable(solution) => {
            for variable in vars {
                let val = solution.get_integer_value(variable) as u32;
                println!("{variable}: {val}");
            }
        },
        SatisfactionResult::Unsatisfiable => panic!("unsat"),
        SatisfactionResult::Unknown => panic!("unknown")
    }
}