use std::collections::hash_map::Entry::Occupied;
use crate::basic_types::StoredConflictInfo;
use crate::engine::conflict_analysis::{AnalysisStep, ConflictAnalyser, ConflictAnalysisContext, ConflictAnalysisResult, LearnedClause, LearnedLinearConstraint, ResolutionConflictAnalyser};
use crate::engine::constraint_satisfaction_solver::CoreExtractionResult;
use crate::engine::propagation::propagator::LinearConstraint;
use crate::propagators::linear_less_or_equal::LinearLessOrEqualPropagator;
use crate::variables::{DomainId, TransformableVariable};
use std::collections::HashMap;

#[derive(Default, Debug)]
pub(crate) struct IntSatConflictAnalyser {
    resolution_analyser: ResolutionConflictAnalyser,
}

fn div_ceil(num: i32, div: i32) -> i32 {
    let d = num / div;
    let r = num % div;
    if (r > 0 && div > 0) || (r < 0 && div < 0) {
        d + 1
    } else {
        d
    }
}

enum CutResult {
    Tautology,
    Overflow,
    Contradiction,
    Success { constraint: LinearConstraint, skip_early_backjump: bool}
}

// TODO tautology
// TODO shaving
fn apply_cut(var: DomainId, c1: &LinearConstraint, c2: &LinearConstraint) -> CutResult {
    let c1_scale = c1.find_variable_scale(var).unwrap().abs();
    let c2_scale = c2.find_variable_scale(var).unwrap().abs();

    let g = gcd(c1_scale, c2_scale);
    let mult_c1 = c1_scale / g;
    let mult_c2 = c2_scale / g;

    let mut skip_early_backjump = true;

    let mut new_lhs: HashMap<_, _> = c1.lhs.iter().map(|(id, scale)| {
        (*id, mult_c1 * *scale)
    }).collect();

    c2.lhs.iter().for_each(|(id, scale)| {
        let entry = new_lhs.entry(*id);

        // Don't skip early backjump in case there is a clash between variables that are not 'var'
        if matches!(entry, Occupied { .. }) && *id != var {
            skip_early_backjump = false;
        }

        let curr_scale = entry.or_insert(0);
        *curr_scale += mult_c2 * scale;

        // If it's fully canceled out, remove it
        if *curr_scale == 0 {
            let _ = new_lhs.remove(id);
        }
    });

    let mut new_rhs = c1.rhs * mult_c1 + c2.rhs * mult_c2;

    if new_lhs.len() == 0 { return CutResult::Contradiction; }

    // Normalization
    let mut new_gcd = new_lhs.iter()
        .map(|(_, scale)| *scale)
        .reduce(|a, b| gcd(a, b)).unwrap_or(new_rhs);
    new_gcd = gcd(new_gcd, new_rhs);

    new_lhs.iter_mut().for_each(|(_, scale)| {
        *scale = div_ceil(*scale, new_gcd);
    });
    new_rhs = div_ceil(new_rhs, new_gcd);

    // Check overflow
    if new_lhs.iter().any(|(_, scale)| *scale > (1<<30)) { return CutResult::Overflow; }
    if new_rhs > (1<<30) { return CutResult::Overflow; }

    CutResult::Success {
        constraint: LinearConstraint {
            lhs: new_lhs.into_iter().collect(),
            rhs: new_rhs,
        },
        skip_early_backjump,
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
                learned_literals: vec![context.assignments_propositional.false_literal],
                backjump_level: 0,
            })
        }

        let mut conflicting_constraint = match context.solver_state.get_conflict_info() {
            StoredConflictInfo::Explanation { conjunction: _conjunction, propagator } => {
                let prop = &context.propagator_store[*propagator];
                prop.get_linear_constraint().unwrap()
            }
            _ => {
                println!("Unsupported conflict, trying resolution");
                return self.resolution_analyser.conflict_analysis(context);
            },
        };

        let current_decision_level = context.assignments_integer.get_decision_level();
        let mut trail_index = context.assignments_integer.num_trail_entries() - 1;

        let mut assignments_curr_state = context.assignments_integer.clone();

        loop {
            println!("Conflicting constraint: {conflicting_constraint}");

            let mut conflicting_constraint_slack = conflicting_constraint.slack(&assignments_curr_state);

            // Find trail entry at which the conflicting constraint is not conflicting anymore
            let trail_entry = loop {
                assignments_curr_state.synchronise_trail_idx(trail_index);

                let trail_entry = assignments_curr_state.get_trail_entry(trail_index);
                let trail_entry_var = trail_entry.predicate.get_domain();

                if trail_entry.reason.is_none() {
                    println!("No reason (e.g. decision / unit propagation), trying resolution");
                    return self.resolution_analyser.conflict_analysis(context);
                }

                // If the conflicting constraint doesn't contain this variable, next level
                let Some(conf_var_scale) = conflicting_constraint.find_variable_scale(trail_entry_var) else {
                    trail_index -= 1;
                    continue;
                };

                let (prev_lb, prev_ub) = assignments_curr_state.prev_bounds(&trail_entry);

                let lower_bound_diff = assignments_curr_state.get_lower_bound(trail_entry_var) - prev_lb;
                let upper_bound_diff = prev_ub - assignments_curr_state.get_upper_bound(trail_entry_var);

                let increases_slack = (conf_var_scale > 0 && lower_bound_diff > 0) || (conf_var_scale < 0 && upper_bound_diff > 0);
                if increases_slack {
                    conflicting_constraint_slack += if lower_bound_diff > 0 {
                        lower_bound_diff * conf_var_scale
                    } else {
                        upper_bound_diff * conf_var_scale
                    };
                }

                trail_index -= 1;

                if conflicting_constraint_slack >= 0 {
                    break trail_entry;
                }
            };

            // Find the scale of the variable of its reason
            let propagator_id = context.reason_store.get_propagator(trail_entry.reason.unwrap());
            let propagator = &context.propagator_store[propagator_id];
            let prop_constraint = propagator.get_linear_constraint().unwrap();
            println!("Propagating constraint conflicting at {trail_index}: {prop_constraint}");

            // VSIDS
            prop_constraint.lhs.iter().for_each(|(id, _)| {
                context
                    .brancher
                    .on_appearance_in_conflict_integer(*id);
            });

            // Actually apply the cut
            let (new_conflicting_constraint, skip_early_backjump) = match apply_cut(trail_entry.predicate.get_domain(), &conflicting_constraint, &prop_constraint) {
                CutResult::Tautology => {
                    println!("Tautology, trying resolution!");
                    return self.resolution_analyser.conflict_analysis(context);
                }
                CutResult::Overflow => {
                    println!("Overflow, trying resolution!");
                    return self.resolution_analyser.conflict_analysis(context);
                }
                CutResult::Contradiction => {
                    println!("Contradiction, unsat!");
                    return ConflictAnalysisResult::CLAUSE(LearnedClause {
                        learned_literals: vec![context.assignments_propositional.false_literal],
                        backjump_level: 0,
                    })
                }
                CutResult::Success { constraint, skip_early_backjump } => (constraint, skip_early_backjump)
            };

            println!("New conflicting constraint after eliminating {:?}: {new_conflicting_constraint}", trail_entry.predicate.get_domain());

            // If this new constraint is not false at the current height, skip!
            if !new_conflicting_constraint.is_conflicting(&assignments_curr_state) {
                println!("Not conflicting, trying resolution!");
                return self.resolution_analyser.conflict_analysis(context);
            }

            conflicting_constraint = new_conflicting_constraint;

            if skip_early_backjump {
                println!("No clash in cuts, skipping early backjump!");
                continue;
            }

            // TODO checkout original implementation
            for backjump_level in (0..current_decision_level).rev() {
                let cloned_assignments = &mut context.assignments_integer.clone();
                let _ = cloned_assignments.synchronise(backjump_level, false, 0);

                let can_propagate_at_level = conflicting_constraint.is_conflicting(cloned_assignments) ||
                    conflicting_constraint.is_propagating(cloned_assignments);

                if can_propagate_at_level {
                    println!("Backtrack to {backjump_level}: {conflicting_constraint}");
                    println!("-------------------");

                    let vars = conflicting_constraint.lhs.iter().map(|(id, scale)| id.scaled(*scale)).collect();

                    return ConflictAnalysisResult::LINEAR(LearnedLinearConstraint {
                        learned_constraint: Box::new(LinearLessOrEqualPropagator::new(vars, conflicting_constraint.rhs)),
                        backjump_level,
                    })
                }
            }
        }
    }

    fn compute_clausal_core(&mut self, context: &mut ConflictAnalysisContext) -> CoreExtractionResult {
        self.resolution_analyser.compute_clausal_core(context)
    }

    fn get_conflict_reasons(&mut self, context: &mut ConflictAnalysisContext, on_analysis_step: &mut dyn FnMut(AnalysisStep)) {
        self.resolution_analyser.get_conflict_reasons(context, on_analysis_step)
    }
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
        let i: i32 = 1 << shift;
        return i.abs();
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