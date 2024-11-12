use std::collections::hash_map::Entry::Occupied;
use crate::basic_types::StoredConflictInfo;
use crate::engine::cp::propagation::linear_constraint::LinearConstraint;
use crate::propagators::linear_less_or_equal::LinearLessOrEqualPropagator;
use crate::variables::{DomainId, TransformableVariable};
use std::collections::HashMap;
use crate::conflict_resolution::{ConflictAnalysisNogoodContext, ConflictResolver, LearnedNogood, ResolutionResolver};
use crate::engine::conflict_analysis::{ConflictResolveResult, LearnedConstraint};
use crate::engine::conflict_analysis::ConflictResolveResult::{Constraint, Nogood};
use crate::predicates::Predicate;

#[derive(Default, Debug)]
pub struct IntSatConflictResolver {
    resolution_resolver: ResolutionResolver,
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
    if new_lhs.iter().any(|(_, scale)| scale.abs() > (1<<30)) { return CutResult::Overflow; }
    if new_rhs.abs() > (1<<30) { return CutResult::Overflow; }

    CutResult::Success {
        constraint: LinearConstraint {
            lhs: new_lhs.into_iter().collect(),
            rhs: new_rhs,
        },
        skip_early_backjump,
    }
}

impl ConflictResolver for IntSatConflictResolver {
    fn resolve_conflict(&mut self, context: &mut ConflictAnalysisNogoodContext) -> Option<ConflictResolveResult> {
        println!("PERFORMING CONFLICT ANALYSIS WITH TRAIL");
        for i in 0..context.assignments.num_trail_entries() {
            let entry = context.assignments.get_trail_entry(i);
            println!("{i} (level {:?}): {:?}", context.assignments.get_decision_level_for_predicate(&entry.predicate), entry.predicate)
        }
        println!("-");

        if context.assignments.get_decision_level() == 0 {
            println!("Level 0 conflict: unsat!");
            return Some(Nogood(LearnedNogood {
                predicates: vec![Predicate::trivially_false()],
                backjump_level: 0,
            }));
        }

        let mut conflicting_constraint = match context.solver_state.get_conflict_info() {
            StoredConflictInfo::Propagator { propagator_id, .. } => {
                let prop = &context.propagators[*propagator_id];
                prop.get_linear_constraint().unwrap()
            }
            _ => {
                // TODO handle this case
                println!("Unsupported conflict, trying resolution");
                return self.resolution_resolver.resolve_conflict(context);
            },
        };

        let current_decision_level = context.assignments.get_decision_level();
        let mut trail_index = context.assignments.num_trail_entries() - 1;

        loop {
            println!("Conflicting constraint: {conflicting_constraint}");

            // Find trail entry at which the conflicting constraint is not conflicting anymore
            let trail_entry = loop {
                let trail_entry = context.assignments.get_trail_entry(trail_index);
                let trail_entry_var = trail_entry.predicate.get_domain();

                if trail_entry.reason.is_none() {
                    // TODO handle this case
                    println!("No reason (e.g. decision / unit propagation), trying resolution");
                    return self.resolution_resolver.resolve_conflict(context);
                }

                // If the conflicting constraint doesn't contain this variable, go to next level
                if !conflicting_constraint.contains_variable(trail_entry_var) {
                    trail_index -= 1;
                    continue;
                };

                if conflicting_constraint.is_conflicting(context.assignments, Some(trail_index)) {
                    break trail_entry;
                }

                trail_index -= 1;
            };

            // Find the scale of the variable of its reason
            let propagator_id = context.reason_store.get_propagator(trail_entry.reason.unwrap());
            let propagator = &context.propagators[propagator_id];
            let prop_constraint = propagator.get_linear_constraint().unwrap();
            println!("Propagating constraint conflicting at {trail_index}: {prop_constraint}");

            // TODO VSIDS

            // Actually apply the cut
            let (new_conflicting_constraint, skip_early_backjump) = match apply_cut(trail_entry.predicate.get_domain(), &conflicting_constraint, &prop_constraint) {
                CutResult::Tautology => {
                    println!("Tautology, trying resolution!");
                    return self.resolution_resolver.resolve_conflict(context);
                }
                CutResult::Overflow => {
                    println!("Overflow, trying resolution!");
                    return self.resolution_resolver.resolve_conflict(context);
                }
                CutResult::Contradiction => {
                    println!("Contradiction, unsat!");
                    return Some(Nogood(LearnedNogood {
                        predicates: vec![Predicate::trivially_false()],
                        backjump_level: 0,
                    }));
                }
                CutResult::Success { constraint, skip_early_backjump } => (constraint, skip_early_backjump)
            };

            println!("New conflicting constraint after eliminating {:?}: {new_conflicting_constraint}", trail_entry.predicate.get_domain());

            // If this new constraint is not false at the current height, skip!
            if !new_conflicting_constraint.is_conflicting(context.assignments, Some(trail_index)) {
                println!("Not conflicting, trying resolution!");
                return self.resolution_resolver.resolve_conflict(context);
            }

            conflicting_constraint = new_conflicting_constraint;

            if skip_early_backjump {
                println!("No clash in cuts, skipping early backjump!");
                continue;
            }

            // TODO checkout original implementation
            for backjump_level in (0..current_decision_level).rev() {
                let trail_level = context.assignments.trail.get_trail_position_for_decision_level(backjump_level);
                let can_propagate_at_level = conflicting_constraint.is_propagating(context.assignments, Some(trail_level));

                if can_propagate_at_level {
                    println!("Backtrack to {backjump_level}: {conflicting_constraint}");
                    println!("-------------------");

                    let vars = conflicting_constraint.lhs.iter().map(|(id, scale)| id.scaled(*scale)).collect();

                    return Some(Constraint(LearnedConstraint {
                        learned_constraint: Box::new(LinearLessOrEqualPropagator::new(vars, conflicting_constraint.rhs)),
                        backjump_level,
                    }));
                }
            }
        }
    }

    fn process(&mut self, context: &mut ConflictAnalysisNogoodContext, learned_nogood: &Option<ConflictResolveResult>) -> Result<(), ()> {
        todo!()
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