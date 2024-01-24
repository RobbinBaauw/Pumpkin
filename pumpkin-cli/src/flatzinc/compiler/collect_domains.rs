//! Compilation phase that builds a map from flatzinc variables to solver domains.

use flatzinc::Annotation;

use super::context::CompilationContext;
use crate::flatzinc::ast::FlatZincAst;
use crate::flatzinc::ast::SingleVarDecl;
use crate::flatzinc::instance::Output;
use crate::flatzinc::FlatZincError;

pub fn run(ast: &FlatZincAst, context: &mut CompilationContext) -> Result<(), FlatZincError> {
    for single_var_decl in &ast.single_variables {
        match single_var_decl {
            SingleVarDecl::Bool { id, annos, .. } => {
                let id = context.identifiers.get_interned(id);

                let representative = context.literal_equivalences.representative(&id);
                let domain = context.literal_equivalences.domain(&id);

                let literal = *context
                    .boolean_variable_map
                    .entry(representative)
                    .or_insert_with(|| domain.into_literal(context.solver));

                if is_output_variable(annos) {
                    context.outputs.push(Output::bool(id, literal));
                }
            }

            SingleVarDecl::IntInRange { id, annos, .. } => {
                let id = context.identifiers.get_interned(id);

                let representative = context.integer_equivalences.representative(&id);
                let domain = context.integer_equivalences.domain(&id);

                let domain_id = *context
                    .integer_variable_map
                    .entry(representative)
                    .or_insert_with(|| {
                        if domain.is_constant() {
                            *context
                                .constant_domain_ids
                                .entry(domain.lb)
                                .or_insert_with(|| domain.into_variable(context.solver))
                        } else {
                            domain.into_variable(context.solver)
                        }
                    });

                if is_output_variable(annos) {
                    context.outputs.push(Output::int(id, domain_id));
                }
            }
        }
    }

    Ok(())
}

fn is_output_variable(annos: &[Annotation]) -> bool {
    annos.iter().any(|ann| ann.id == "output_var")
}
