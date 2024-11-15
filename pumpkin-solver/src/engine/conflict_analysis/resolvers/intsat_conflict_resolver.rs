use std::collections::hash_map::Entry::Occupied;
use crate::basic_types::StoredConflictInfo;
use crate::engine::cp::propagation::linear_less_or_equal::LinearLessOrEqual;
use crate::propagators::linear_less_or_equal::LinearLessOrEqualPropagator;
use crate::variables::{DomainId, TransformableVariable};
use std::collections::HashMap;
use log::{debug, trace};
use crate::basic_types::moving_averages::MovingAverage;
use crate::conflict_resolution::{ConflictAnalysisContext, ConflictResolver, LearnedNogood, ResolutionResolver};
use crate::engine::conflict_analysis::{ConflictResolveResult, LearnedConstraint};
use crate::engine::conflict_analysis::ConflictResolveResult::{Constraint, Nogood};
use crate::engine::propagation::PropagatorInitialisationContext;
use crate::predicates::Predicate;
use crate::pumpkin_assert_ne_simple;

#[derive(Debug, Default)]
pub struct IntSatConflictResolver {
    resolution_resolver: ResolutionResolver,
}

impl IntSatConflictResolver {
    pub fn new(only_propagate: bool) -> Self {
        let mut resolver = ResolutionResolver::default();
        resolver.only_propagate = only_propagate;
        IntSatConflictResolver { resolution_resolver: resolver }
    }
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
    NothingLearned,
    Overflow,
    Contradiction,
    Success { inequality: LinearLessOrEqual, skip_early_backjump: bool}
}

// TODO tautology
// TODO shaving
fn apply_cut(var: DomainId, c1: &LinearLessOrEqual, c2: &LinearLessOrEqual) -> CutResult {
    let c1_scale = c1.find_variable_scale(var).unwrap().abs();
    let c2_scale = c2.find_variable_scale(var).unwrap().abs();

    let g = gcd(c1_scale, c2_scale);
    let mult_c1 = c1_scale / g;
    let mult_c2 = c2_scale / g;

    let mut skip_early_backjump = true;

    let mut new_lhs: HashMap<DomainId, i32> = HashMap::new();

    for (id, scale) in c1.lhs.iter() {
        let Some(new_scale) = mult_c1.checked_mul(*scale) else { return CutResult::Overflow; };
        let _ = new_lhs.insert(*id, new_scale);
    }

    for (id, scale) in c2.lhs.iter() {
        let entry = new_lhs.entry(*id);

        // Don't skip early backjump in case there is a clash between variables that are not 'var'
        if matches!(entry, Occupied { .. }) && *id != var {
            skip_early_backjump = false;
        }

        let curr_scale = entry.or_insert(0);
        let Some(new_scale) = mult_c2.checked_mul(*scale) else { return CutResult::Overflow; };
        *curr_scale += new_scale;

        if *curr_scale == 0 {
            let _ = new_lhs.remove(&id);
        }
    }

    let Some(c1_rhs_scaled) = c1.rhs.checked_mul(mult_c1) else { return CutResult::Overflow; };
    let Some(c2_rhs_scaled) = c2.rhs.checked_mul(mult_c2) else { return CutResult::Overflow; };
    let Some(mut new_rhs) = c1_rhs_scaled.checked_add(c2_rhs_scaled) else { return CutResult::Overflow };

    if new_lhs.len() == 0 {
        return if new_rhs < 0 {
            CutResult::Contradiction
        } else {
            CutResult::NothingLearned
        }
    }

    // Normalization
    let mut new_gcd = new_lhs.iter()
        .map(|(_, scale)| *scale)
        .reduce(|a, b| gcd(a, b)).unwrap_or(new_rhs);
    new_gcd = gcd(new_gcd, new_rhs);

    new_lhs.iter_mut().for_each(|(_, scale)| {
        *scale = div_ceil(*scale, new_gcd);
    });
    new_rhs = div_ceil(new_rhs, new_gcd);

    CutResult::Success {
        inequality: LinearLessOrEqual {
            lhs: new_lhs.into_iter().collect(),
            rhs: new_rhs,
        },
        skip_early_backjump,
    }
}

impl ConflictResolver for IntSatConflictResolver {
    fn resolve_conflict(&mut self, context: &mut ConflictAnalysisContext) -> Option<ConflictResolveResult> {
        if context.is_completing_proof {
            // TODO implement this for intsat
            debug!("==> Completing proof, trying resolution");
            context.counters.intsat_statistics.intsat_fallback_used += 1;
            return self.resolution_resolver.resolve_conflict(context);
        }

        trace!("PERFORMING CONFLICT ANALYSIS WITH TRAIL");
        for i in 0..context.assignments.num_trail_entries() {
            let entry = context.assignments.get_trail_entry(i);
            let prop_name = if entry.reason.is_some() {
                let propagator_id = context.reason_store.get_propagator(entry.reason.unwrap());
                &context.propagators[propagator_id].name()
            } else {
                "decision"
            };

            trace!("{i} (level {:?}): {:?} ({prop_name})", context.assignments.get_decision_level_for_predicate(&entry.predicate).unwrap(), entry.predicate)
        }

        pumpkin_assert_ne_simple!(context.assignments.get_decision_level(), 0);

        let mut conflicting_constraint = match context.solver_state.get_conflict_info() {
            StoredConflictInfo::Propagator { propagator_id, .. } => {
                let propagator = &context.propagators[propagator_id];

                match propagator.linear_inequality_explanation() {
                    None => {
                        debug!("==> Conflict caused by propagator that cannot explain with linear inequality, trying resolution");
                        context.counters.intsat_statistics.intsat_fallback_used += 1;
                        return self.resolution_resolver.resolve_conflict(context);
                    }
                    Some(prop_constraint_expl) => prop_constraint_expl
                }
            }
            StoredConflictInfo::EmptyDomain { .. } => {
                let last_entry = context.assignments.get_last_entry_on_trail();
                let propagator_id = context.reason_store.get_propagator(last_entry.reason.unwrap());
                let propagator = &context.propagators[propagator_id];

                match propagator.linear_inequality_explanation() {
                    None => {
                        debug!("==> Empty domain caused by propagator that cannot explain with linear inequality, trying resolution");
                        context.counters.intsat_statistics.intsat_fallback_used += 1;
                        return self.resolution_resolver.resolve_conflict(context);
                    }
                    Some(prop_constraint_expl) => prop_constraint_expl
                }
            },
            StoredConflictInfo::RootLevelConflict(..) => unreachable!("Shouldn't be possible"),
        };

        let current_decision_level = context.assignments.get_decision_level();
        let mut trail_index = context.assignments.num_trail_entries() - 1;

        loop {
            debug!("========");
            debug!("Conflicting constraint: {conflicting_constraint}");

            // Find trail entry at which the conflicting constraint is not conflicting anymore
            let trail_entry = loop {
                debug!("==> Finding trail entry at level {trail_index}");

                let trail_entry = context.assignments.get_trail_entry(trail_index);
                let trail_entry_var = trail_entry.predicate.get_domain();

                if trail_entry.reason.is_none() {
                    // When a decision is reached, and we haven't found a conflicting solution yet, skip
                    debug!("==>==> Decision reached, trying resolution");
                    context.counters.intsat_statistics.intsat_fallback_used += 1;
                    return self.resolution_resolver.resolve_conflict(context);
                }

                // If the conflicting constraint doesn't contain this variable, go to next level
                if !conflicting_constraint.contains_variable(trail_entry_var) {
                    debug!("==>==> Not containing {trail_entry_var} at {trail_index}, skip");
                    trail_index -= 1;
                    continue;
                };

                if conflicting_constraint.is_conflicting(context.assignments, Some(trail_index)) {
                    trail_index -= 1;
                    break trail_entry;
                }

                debug!("==>==> Not conflicting at {trail_index}, skip");
                trail_index -= 1;
            };

            // Find the scale of the variable of its reason
            let propagator_id = context.reason_store.get_propagator(trail_entry.reason.unwrap());
            let propagator = &context.propagators[propagator_id];

            let prop_constraint_expl_opt = propagator.linear_inequality_explanation();
            if prop_constraint_expl_opt.is_none() {
                // In this case, we have a conjunction of predicates, which we can somewhat turn into a linear constraint
                // Say for instance, our conflicting constraint is 3x + y <= 3, with reason for [y >= 3] being [z >= 2] /\ [y <= 2]
                // This can be turned into a linear constraint [z <= 1] + [y >= 3] >= 1
                // However, we cannot apply cancelling addition between y and [y >= 3]...

                // IntSat: just propagates a bound when performing resolution
                // Pumpkin: propagates a nogood conjunction as well, so this step will come up quite often.
                //          Maybe should first work with a mode that doesn't learn anything

                // Emir's idea: You can represent 3x + y <= 3 (with x, y in [0, 3]) as
                // 3 * [x >= 1] + [y >= 1] + [y >= 2] + [y >= 3] <= 3

                // We can then apply resolution by inverting our nogoods: -[z <= 1] + -[y >= 3] <= -1
                // This gives 3 * [x >= 1] + [y >= 1] + [y >= 2] - [z <= 1] <= 2
                // This is correct in that it doesn't discard any feasible solutions (it allows any combination of y, z when x = 0)

                // The main problem here is that a linear constraint is in the form <=, and a clause >= 1, so we have to invert it to get to the same shape
                // Then, it _should_ work the same and the fields always have opposite signs

                // Alternatively: When performing resolution, we can store the conflicting linear constraint of this conflict as being the reason.
                // The next time we encounter the propagated nogood, we can use the linear constraint.
                // However, this still just uses resolution, but allows for using linear constraints in some more cases, even when nogoods have been propagated
                // TODO think this through a bit more to see if it's nice

                debug!("==>==> Detected nogoods, performing resolution");
                context.counters.intsat_statistics.intsat_fallback_used += 1;
                return self.resolution_resolver.resolve_conflict(context);
            }

            let prop_constraint_expl = prop_constraint_expl_opt.unwrap();
            debug!("==>==> Merging with {:?}: {prop_constraint_expl}", trail_entry.predicate.get_domain());

            // Note to self: it's not required to check whether we need to invert when only dealing with linear inequalities
            // You can only get a conflict if the propagation is made for the same variable with a different sign
            // If it "helps" the other constraint increase its slack, it will never cause a conflict
            let (new_conflicting_constraint, skip_early_backjump) = match apply_cut(trail_entry.predicate.get_domain(), &conflicting_constraint, &prop_constraint_expl) {
                CutResult::NothingLearned => {
                    debug!("==>==> Nothing learned, trying resolution!");
                    context.counters.intsat_statistics.intsat_fallback_used += 1;
                    return self.resolution_resolver.resolve_conflict(context);
                }
                CutResult::Overflow => {
                    debug!("==>==> Overflow, trying resolution!");
                    context.counters.intsat_statistics.intsat_fallback_used += 1;
                    return self.resolution_resolver.resolve_conflict(context);
                }
                CutResult::Contradiction => {
                    debug!("==>==> Contradiction, unsat!");
                    return Some(Nogood(LearnedNogood {
                        predicates: vec![Predicate::trivially_true()],
                        backjump_level: 0,
                    }));
                }
                CutResult::Success { inequality: constraint, skip_early_backjump } => (constraint, skip_early_backjump)
            };

            debug!("==> New conflicting constraint after eliminating {:?}: {new_conflicting_constraint}", trail_entry.predicate.get_domain());

            // If this new constraint is not false at the current height, skip!
            if !new_conflicting_constraint.is_conflicting(context.assignments, Some(trail_index)) {
                debug!("==> Not conflicting, trying resolution!");
                context.counters.intsat_statistics.intsat_fallback_used += 1;
                return self.resolution_resolver.resolve_conflict(context);
            }

            conflicting_constraint = new_conflicting_constraint;

            if skip_early_backjump {
                debug!("==> No clash in cuts, skipping early backjump check!");
                continue;
            }

            // TODO checkout original implementation
            for backjump_level in (0..current_decision_level).rev() {
                let trail_level = context.assignments.trail.get_trail_position_for_decision_level(backjump_level);

                let is_propagating_or_false = conflicting_constraint.is_propagating(context.assignments, Some(trail_level)) ||
                    conflicting_constraint.is_conflicting(context.assignments, Some(trail_level));

                debug!("==> Checking decision/trail level ({backjump_level}/{trail_level}) for propagation/false: {is_propagating_or_false}");

                if is_propagating_or_false {
                    debug!("==> Backtrack to {backjump_level}: {conflicting_constraint}");

                    // Running resolution resolver to update activities
                    // Ignore the result
                    let _ = self.resolution_resolver.resolve_conflict(context);

                    context.counters.intsat_statistics.intsat_learned_constraints += 1;
                    context.counters.intsat_statistics.intsat_learned_constraints_avg_length.add_term(conflicting_constraint.lhs.len() as u64);
                    context.counters.intsat_statistics.intsat_constraint_avg_lhs_coeff.add_term(conflicting_constraint.lhs.iter().map(|(_, scale)| scale.abs()).max().unwrap() as u64);

                    let vars = conflicting_constraint.lhs.iter().map(|(id, scale)| id.scaled(*scale)).collect();

                    return Some(Constraint(LearnedConstraint {
                        learned_constraint: Box::new(LinearLessOrEqualPropagator::new(vars, conflicting_constraint.rhs)),
                        backjump_level,
                    }));
                }
            }
        }
    }

    fn process(&mut self, context: &mut ConflictAnalysisContext, resolve_result: &Option<ConflictResolveResult>) -> Result<(), ()> {
        let resolve_result_unwrap = resolve_result.as_ref().expect("Expected nogood / constraint");

        let Constraint(learned_constraint) = resolve_result_unwrap else {
            return self.resolution_resolver.process(context, resolve_result);
        };

        context.backtrack(learned_constraint.backjump_level);

        let new_propagator_id = context.propagators.alloc(learned_constraint.learned_constraint.clone(), None);
        let new_propagator = &mut context.propagators[new_propagator_id];

        let mut initialisation_context = PropagatorInitialisationContext::new(
            &mut context.watch_list_cp,
            new_propagator_id,
            &context.assignments,
        );

        let initialisation_status = new_propagator.initialise_at_root(&mut initialisation_context);
        if initialisation_status.is_err() {
            Err(())
        } else {
            context.propagator_queue.enqueue_propagator(new_propagator_id, new_propagator.priority());
            Ok(())
        }
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