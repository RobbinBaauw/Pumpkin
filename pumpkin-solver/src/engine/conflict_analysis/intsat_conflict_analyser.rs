use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use itertools::Itertools;
use crate::basic_types::StoredConflictInfo;
use crate::engine::conflict_analysis::{AnalysisStep, ConflictAnalyser, ConflictAnalysisContext, ConflictAnalysisResult, LearnedClause, LearnedLinearConstraint, ResolutionConflictAnalyser};
use crate::engine::constraint_satisfaction_solver::CoreExtractionResult;
use crate::engine::propagation::PropagatorId;
use crate::engine::propagation::store::PropagatorStore;
use crate::propagators::linear_less_or_equal::{can_propagate, LinearLessOrEqualPropagator};
use crate::variables::{DomainId, TransformableVariable};

struct LinearConstraint {
    lhs: Vec<(u32, i32)>,
    rhs: i32,
}

impl Display for LinearConstraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let lhs_mapped = self.lhs.iter().filter_map(|(v, s)| {
            return if *s == 0 {
                None
            } else if *s == 1 {
                Some(format!("x{v}"))
            } else if *s == -1 {
                Some(format!("-x{v}"))
            } else {
                Some(format!("{s}x{v}"))
            }
        }).join(" + ");
        write!(f, "{lhs_mapped} <= {:?}", self.rhs)
    }
}

#[derive(Default, Debug)]
pub(crate) struct IntSatConflictAnalyser {
    resolution_analyser: ResolutionConflictAnalyser,
}

fn prop_to_linear_constraint(propagator_store: &PropagatorStore, id: PropagatorId) -> LinearConstraint {
    let prop = &propagator_store[id];

    if let Some((x, c)) = prop.get_linear_constraint() {
        let var_c = x.iter().map(|var| var.offset).sum::<i32>();

        let lhs = x.iter().map(|var| (var.id, var.scale)).collect_vec();
        let rhs = c - var_c;

        LinearConstraint { lhs, rhs }
    } else {
        todo!("unsupported propagator")
    }
}

impl ConflictAnalyser for IntSatConflictAnalyser {
    fn conflict_analysis(&mut self, context: &mut ConflictAnalysisContext) -> ConflictAnalysisResult {
        println!("PERFORMING CONFLICT ANALYSIS WITH TRAIL");
        for i in 0..context.assignments_integer.num_trail_entries() {
            let entry = context.assignments_integer.get_trail_entry(i);
            println!("{i} (level {:?}): {:?}", context.assignments_integer.get_decision_level_at_idx(i), entry.predicate)
        }
        println!("-");

        if context.assignments_integer.get_decision_level() == 0 {
            println!("Level 0 conflict: unsat!");
            return ConflictAnalysisResult::CLAUSE(LearnedClause {
                learned_literals: vec![],
                backjump_level: 0,
            })
        }

        let mut conflicting_constraint = match context.solver_state.get_conflict_info() {
            StoredConflictInfo::Explanation { conjunction: _conjunction, propagator } => {
                prop_to_linear_constraint(context.propagator_store, *propagator)
            }
            _ => todo!("unsupported conflict"),
        };

        let current_decision_level = context.assignments_integer.get_decision_level();

        let start_index = context.assignments_integer.num_trail_entries();

        for trail_index in (0..start_index).rev() {
            println!("Conflicting constraint: {conflicting_constraint}");

            let next_literal = context.assignments_integer.get_trail_entry(trail_index);
            let next_literal_id = next_literal.predicate.get_domain();
            let next_literal_reason = next_literal.reason.unwrap();
            println!("Next literal: {next_literal_id}");

            if conflicting_constraint.lhs.iter().find(|(var, scale)| {
                *var == next_literal_id.id && *scale != 0
            }).is_none() {
                println!("Literal not in conflicting constraint");
                continue;
            }

            // Find the variable in the current conflicting constraint corresponding to this trail entry
            // If it does not exist, it means this trail entry is not relevant
            let conflicting_var = conflicting_constraint.lhs.iter().find(|(id, _)| *id == next_literal_id.id);
            if conflicting_var.is_none() { continue; }

            let (_, conflicting_scale) = conflicting_var.unwrap();

            // Find the scale of the variable of its reason
            let propagator = context.reason_store.get_propagator(next_literal_reason);
            let prop_constraint = prop_to_linear_constraint(context.propagator_store, propagator);
            println!("Propagating constraint: {prop_constraint}");

            let (_, prop_scale) = prop_constraint.lhs.iter().find(|(id, _)| *id == next_literal_id.id).unwrap();

            // Compute the multiplier which to multiply both sides with
            let lcm_val = lcm(*conflicting_scale, *prop_scale);
            let mult_conf = -lcm_val / conflicting_scale;
            let mult_prop = lcm_val / prop_scale;

            // Multiply the conflicting & propagating constraint
            let mut new_lhs: HashMap<_, _> = conflicting_constraint.lhs.iter().map(|(id, scale)| {
                (id, mult_conf * *scale)
            }).collect();

            prop_constraint.lhs.iter().for_each(|(id, scale)| {
                if !new_lhs.contains_key(id) { new_lhs.insert(id, 0); }

                let curr_scale = new_lhs.get_mut(id).unwrap();
                *curr_scale += mult_prop * scale;
            });

            let new_lhs_vec = new_lhs.iter().filter_map(|(id, scale)| {
                if *scale == 0 { None }
                else { Some((**id, *scale)) }
            }).collect_vec();

            let new_lhs_vars = new_lhs_vec.iter().map(|(id, scale)| {
                DomainId::new(*id).scaled(*scale)
            }).collect_vec();

            let new_rhs = conflicting_constraint.rhs * mult_conf + prop_constraint.rhs * mult_prop;

            let new_conflicting_constraint = LinearConstraint { lhs: new_lhs_vec, rhs: new_rhs };

            let cloned_assignments = &mut context.assignments_integer.clone();
            for backjump_level in (0..current_decision_level).rev() {
                let _ = cloned_assignments.synchronise(backjump_level, false, 0);

                let can_propagate_at_level = can_propagate(cloned_assignments, &new_lhs_vars, new_rhs);
                if can_propagate_at_level {
                    println!("Backtrack to {backjump_level}: {new_conflicting_constraint}");
                    println!("-------------------");
                    return ConflictAnalysisResult::LINEAR(LearnedLinearConstraint {
                        learned_constraint: Box::new(LinearLessOrEqualPropagator::new(new_lhs_vars.into_boxed_slice(), new_rhs)),
                        backjump_level,
                    })
                }
            }

            println!("New conflicting constraint: {new_conflicting_constraint}");

            conflicting_constraint = new_conflicting_constraint;
        }

        println!("FALLBACK");

        // Perform resolution as backup
        self.resolution_analyser.conflict_analysis(context)
    }

    fn compute_clausal_core(&mut self, context: &mut ConflictAnalysisContext) -> CoreExtractionResult {
        self.resolution_analyser.compute_clausal_core(context)
    }

    fn get_conflict_reasons(&mut self, context: &mut ConflictAnalysisContext, on_analysis_step: &mut dyn FnMut(AnalysisStep)) {
        self.resolution_analyser.get_conflict_reasons(context, on_analysis_step)
    }
}

#[inline]
fn lcm(a: i32, b: i32) -> i32 {
    if a == 0 && b == 0 { return 0; }
    let gcd = gcd(a, b);
    let lcm = (a * (b / gcd)).abs();
    lcm
}

#[inline]
fn gcd(a: i32, b: i32) -> i32 {
    let mut m = a;
    let mut n = b;
    if m == 0 || n == 0 {
        return (m | n).abs();
    }

    // find common factors of 2
    let shift = (m | n).trailing_zeros();

    // The algorithm needs positive numbers, but the minimum value
    // can't be represented as a positive one.
    // It's also a power of two, so the gcd can be
    // calculated by bitshifting in that case

    // Assuming two's complement, the number created by the shift
    // is positive for all numbers except gcd = abs(min value)
    // The call to .abs() causes a panic in debug mode
    if m == i32::MIN || n == i32::MIN {
        return ((1 << shift) as i32).abs();
    }

    // guaranteed to be positive now, rest like unsigned algorithm
    m = m.abs();
    n = n.abs();

    // divide n and m by 2 until odd
    m >>= m.trailing_zeros();
    n >>= n.trailing_zeros();

    while m != n {
        if m > n {
            m -= n;
            m >>= m.trailing_zeros();
        } else {
            n -= m;
            n >>= n.trailing_zeros();
        }
    }
    m << shift
}