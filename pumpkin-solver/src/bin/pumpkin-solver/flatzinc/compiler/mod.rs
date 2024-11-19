mod collect_domains;
mod context;
mod create_objective;
mod create_search_strategy;
mod define_constants;
mod define_variable_arrays;
mod handle_set_in;
mod merge_equivalences;
mod post_constraints;
mod prepare_variables;

use std::rc::Rc;
use itertools::all;
use context::CompilationContext;
use pumpkin_solver::Solver;

use super::ast::FlatZincAst;
use super::instance::{FlatZincInstance, Output, VariableOutput};
use super::FlatZincError;
use super::FlatZincOptions;

pub(crate) fn compile(
    ast: FlatZincAst,
    solver: &mut Solver,
    options: FlatZincOptions,
) -> Result<FlatZincInstance, FlatZincError> {
    let mut context = CompilationContext::new(solver);

    define_constants::run(&ast, &mut context)?;
    prepare_variables::run(&ast, &mut context)?;
    merge_equivalences::run(&ast, &mut context)?;
    handle_set_in::run(&ast, &mut context)?;
    collect_domains::run(&ast, &mut context)?;
    define_variable_arrays::run(&ast, &mut context)?;
    post_constraints::run(&ast, &mut context, options)?;
    let objective_function = create_objective::run(&ast, &mut context)?;
    let search = create_search_strategy::run(&ast, &mut context)?;

    let mut all_variables: Vec<Output> = vec![];

    context.boolean_variable_map.iter()
        .for_each(|(id, lit)| all_variables.push(Output::bool(id.clone(), *lit)));

    context.integer_variable_map.iter()
        .for_each(|(id, int)| all_variables.push(Output::int(id.clone(), *int)));

    // TODO improve
    // context.boolean_variable_arrays.iter()
    //     .for_each(|(id, ints)| all_variables.push(Output::array_of_bool(id.clone(), *ints)));
    //
    // context.integer_variable_arrays.iter()
    //     .for_each(|(id, ints)| all_variables.push(Output::array_of_int(id.clone(), Box::new(), Rc::clone(ints))));

    Ok(FlatZincInstance {
        all_variables,
        outputs: context.outputs,
        objective_function,
        search: Some(search),
    })
}
