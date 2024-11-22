use std::collections::hash_map::Entry::Occupied;
use std::collections::HashMap;

use log::debug;

use crate::basic_types::moving_averages::MovingAverage;
use crate::basic_types::StoredConflictInfo;
use crate::engine::conflict_analysis::ConflictAnalysisContext;
use crate::engine::conflict_analysis::ConflictResolveResult;
use crate::engine::conflict_analysis::ConflictResolveResult::Constraint;
use crate::engine::conflict_analysis::ConflictResolveResult::Nogood;
use crate::engine::conflict_analysis::ConflictResolver;
use crate::engine::conflict_analysis::LearnedConstraint;
use crate::engine::conflict_analysis::LearnedNogood;
use crate::engine::cp::propagation::linear_less_or_equal::LinearLessOrEqual;
use crate::engine::propagation::PropagatorInitialisationContext;
use crate::engine::ResolutionResolver;
use crate::predicates::Predicate;
use crate::propagators::linear_less_or_equal::LinearLessOrEqualPropagator;
use crate::pumpkin_assert_ne_simple;
use crate::variables::DomainId;

#[derive(Debug, Default)]
pub struct IntSatConflictResolver {
    resolution_resolver: ResolutionResolver,
}

enum CutResult {
    NothingLearned,
    Overflow,
    Contradiction,
    Success {
        inequality: LinearLessOrEqual,
        skip_early_backjump: bool,
    },
}

impl IntSatConflictResolver {
    fn apply_fallback(
        &mut self,
        context: &mut ConflictAnalysisContext,
        reason: &str,
    ) -> Option<ConflictResolveResult> {
        debug!("==>==> {reason}, trying resolution!");
        context.counters.intsat_statistics.intsat_fallback_used += 1;
        self.resolution_resolver.resolve_conflict(context)
    }

    fn apply_cut(var: DomainId, c1: &LinearLessOrEqual, c2: &LinearLessOrEqual) -> CutResult {
        let c1_scale = c1.find_variable_scale(var).unwrap().abs();
        let c2_scale = c2.find_variable_scale(var).unwrap().abs();

        let g = gcd(c1_scale, c2_scale);
        let mult_c1 = c1_scale / g;
        let mult_c2 = c2_scale / g;

        let mut skip_early_backjump = true;

        let mut new_lhs: HashMap<DomainId, i32> = HashMap::new();

        for (id, scale) in c1.lhs.iter() {
            let Some(new_scale) = mult_c1.checked_mul(*scale) else {
                return CutResult::Overflow;
            };
            let _ = new_lhs.insert(*id, new_scale);
        }

        for (id, scale) in c2.lhs.iter() {
            let entry = new_lhs.entry(*id);

            // Don't skip early backjump in case there is a clash between variables that are not
            // 'var'
            if matches!(entry, Occupied { .. }) && *id != var {
                skip_early_backjump = false;
            }

            let Some(new_scale) = mult_c2.checked_mul(*scale) else {
                return CutResult::Overflow;
            };

            let curr_scale = entry.or_insert(0);
            let Some(curr_scale_safe) = curr_scale.checked_add(new_scale) else {
                return CutResult::Overflow;
            };
            *curr_scale = curr_scale_safe;

            if *curr_scale == 0 {
                let _ = new_lhs.remove(&id);
            }
        }

        let Some(c1_rhs_scaled) = c1.rhs.checked_mul(mult_c1) else {
            return CutResult::Overflow;
        };
        let Some(c2_rhs_scaled) = c2.rhs.checked_mul(mult_c2) else {
            return CutResult::Overflow;
        };
        let Some(mut new_rhs) = c1_rhs_scaled.checked_add(c2_rhs_scaled) else {
            return CutResult::Overflow;
        };

        if new_lhs.len() == 0 {
            return if new_rhs < 0 {
                CutResult::Contradiction
            } else {
                CutResult::NothingLearned
            };
        }

        // Normalization
        let mut new_gcd = new_lhs
            .iter()
            .map(|(_, scale)| *scale)
            .reduce(|a, b| gcd(a, b))
            .unwrap_or(new_rhs);
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
}

impl ConflictResolver for IntSatConflictResolver {
    fn resolve_conflict(
        &mut self,
        context: &mut ConflictAnalysisContext,
    ) -> Option<ConflictResolveResult> {
        if context.is_completing_proof {
            // TODO implement this for intsat
            return self.apply_fallback(context, "Completing proof");
        }

        pumpkin_assert_ne_simple!(context.assignments.get_decision_level(), 0);

        let mut conflicting_constraint = match context.solver_state.get_conflict_info() {
            StoredConflictInfo::Propagator { propagator_id, .. } => {
                let propagator = &context.propagators[propagator_id];

                match propagator.linear_inequality_explanation() {
                    None => {
                        return self.apply_fallback(context, "Conflict caused by propagator that cannot explain with linear inequality");
                    }
                    Some(prop_constraint_expl) => prop_constraint_expl,
                }
            }
            StoredConflictInfo::EmptyDomain { .. } => {
                let last_entry = context.assignments.get_last_entry_on_trail();

                // Solver#L1203 has a temporary solution that removes the last tail element, so the
                // reason could also be a decision In that case, we cannot know
                // which propagator actually propagated the element causing the empty domain...
                // So for now, revert to resolution instead. TODO change this back when the
                // workaround is removed
                let Some(last_entry_reason) = last_entry.reason else {
                    return self.apply_fallback(context, "Empty domain because of decision");
                };

                let propagator_id = context.reason_store.get_propagator(last_entry_reason);
                let propagator = &context.propagators[propagator_id];

                match propagator.linear_inequality_explanation() {
                    None => {
                        return self.apply_fallback(context, "Empty domain caused by propagator that cannot explain with linear inequality");
                    }
                    Some(prop_constraint_expl) => prop_constraint_expl,
                }
            }
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

                // When a decision is reached, and we haven't found a conflicting solution yet, skip
                if trail_entry.reason.is_none() {
                    return self.apply_fallback(context, "Decision reached");
                }

                // If the conflicting constraint doesn't contain this variable, go to next level
                if !conflicting_constraint.contains_variable(trail_entry_var) {
                    debug!("==>==> Not containing {trail_entry_var} at {trail_index}, skip");
                    trail_index -= 1;
                    continue;
                };

                if conflicting_constraint.is_conflicting(context.assignments, trail_index) {
                    trail_index -= 1;
                    break trail_entry;
                }

                debug!("==>==> Not conflicting at {trail_index}, skip");
                trail_index -= 1;
            };

            // Find the scale of the variable of its reason
            let propagator_id = context
                .reason_store
                .get_propagator(trail_entry.reason.unwrap());
            let propagator = &context.propagators[propagator_id];

            let prop_constraint_expl_opt = propagator.linear_inequality_explanation();
            if prop_constraint_expl_opt.is_none() {
                // In this case, we have a conjunction of predicates, which we can somewhat turn
                // into a linear constraint Say for instance, our conflicting
                // constraint is 3x + y <= 3, with reason for [y >= 3] being [z >= 2] /\ [y <= 2]
                // This can be turned into a linear constraint [z <= 1] + [y >= 3] >= 1
                // However, we cannot apply cancelling addition between y and [y >= 3]...

                // IntSat: just propagates a bound when performing resolution
                // Pumpkin: propagates a nogood conjunction as well, so this step will come up quite
                // often.          Maybe should first work with a mode that doesn't
                // learn anything

                // Emir's idea: You can represent 3x + y <= 3 (with x, y in [0, 3]) as
                // 3 * [x >= 1] + [y >= 1] + [y >= 2] + [y >= 3] <= 3

                // We can then apply resolution by inverting our nogoods: -[z <= 1] + -[y >= 3] <=
                // -1 This gives 3 * [x >= 1] + [y >= 1] + [y >= 2] - [z <= 1] <= 2
                // This is correct in that it doesn't discard any feasible solutions (it allows any
                // combination of y, z when x = 0)

                // The main problem here is that a linear constraint is in the form <=, and a clause
                // >= 1, so we have to invert it to get to the same shape
                // Then, it _should_ work the same and the fields always have opposite signs

                // Alternatively: When performing resolution, we can store the conflicting linear
                // constraint of this conflict as being the reason. The next time we
                // encounter the propagated nogood, we can use the linear constraint.
                // However, this still just uses resolution, but allows for using linear constraints
                // in some more cases, even when nogoods have been propagated
                // TODO think this through a bit more to see if it's nice

                return self.apply_fallback(context, "Detected nogoods");
            }

            let prop_constraint_expl = prop_constraint_expl_opt.unwrap();
            debug!(
                "==>==> Merging with {:?}: {prop_constraint_expl}",
                trail_entry.predicate.get_domain()
            );

            // Note to self: it's not required to check whether we need to invert when only dealing
            // with linear inequalities You can only get a conflict if the propagation
            // is made for the same variable with a different sign If it "helps" the
            // other constraint increase its slack, it will never cause a conflict
            let (new_conflicting_constraint, skip_early_backjump) = match Self::apply_cut(
                trail_entry.predicate.get_domain(),
                &conflicting_constraint,
                &prop_constraint_expl,
            ) {
                CutResult::NothingLearned => {
                    return self.apply_fallback(context, "Nothing learned");
                }
                CutResult::Overflow => {
                    return self.apply_fallback(context, "Overflow");
                }
                CutResult::Contradiction => {
                    debug!("==>==> Contradiction, unsat!");
                    return Some(Nogood(LearnedNogood {
                        predicates: vec![Predicate::trivially_true()],
                        backjump_level: 0,
                    }));
                }
                CutResult::Success {
                    inequality: constraint,
                    skip_early_backjump,
                } => (constraint, skip_early_backjump),
            };

            debug!("==> New conflicting constraint after eliminating {:?}: {new_conflicting_constraint}", trail_entry.predicate.get_domain());

            // Super inefficient, but necessary...
            // TODO maybe cache?
            if new_conflicting_constraint.overflows(context.assignments, trail_index) {
                return self.apply_fallback(context, "Overflow");
            }

            // If this new constraint is not false at the current height, skip!
            if !new_conflicting_constraint.is_conflicting(context.assignments, trail_index) {
                return self.apply_fallback(context, "Not conflicting");
            }

            conflicting_constraint = new_conflicting_constraint;

            if skip_early_backjump {
                debug!("==> No clash in cuts, skipping early backjump check!");
                continue;
            }

            // IntSat:
            // - find the bounds to undo: all bounds in the trail about the variables in the learned
            //   constraint
            // - sort bounds by heights
            // - if at the current level, it is false (or asserting), update the lowest level &
            //   whether it's asserting there. Continue until all bounds have been popped
            // - if it's still conflicting: return decision level 0
            // - if it's still asserting: return decision level 0
            // - if it's at the current DL: return -1
            // - otherwise, return found values

            // We mimic this by going from 0..current level and checking whether our constraint is
            // conflicting/propagating at that level
            for backjump_level in 0..current_decision_level {
                // The get_trail_position_for_decision_level(backjump_level) returns the length of
                // the trail including the entire backjump level This means we will
                // be jumping to one index lower
                let backjump_trail_level = context
                    .assignments
                    .trail
                    .get_trail_position_for_decision_level(backjump_level)
                    - 1;

                // Super inefficient, but necessary...
                // TODO maybe cache?
                if conflicting_constraint.overflows(context.assignments, backjump_trail_level) {
                    return self.apply_fallback(context, "Overflow");
                }

                let is_propagating = conflicting_constraint
                    .is_propagating(context.assignments, backjump_trail_level);
                let is_false = conflicting_constraint
                    .is_conflicting(context.assignments, backjump_trail_level);
                debug!("==> Checking decision/trail level ({backjump_level}/{backjump_trail_level}) for propagation/false: {is_propagating}/{is_false}");

                if is_propagating || is_false {
                    debug!(
                        "==> Intending to backtrack to {backjump_level}: {conflicting_constraint}"
                    );

                    // Running resolution resolver to update activities
                    // Ignore the result
                    let _ = self.resolution_resolver.resolve_conflict(context);

                    context
                        .counters
                        .intsat_statistics
                        .intsat_learned_constraints += 1;
                    context
                        .counters
                        .intsat_statistics
                        .intsat_learned_constraints_avg_length
                        .add_term(conflicting_constraint.lhs.len() as u64);
                    context
                        .counters
                        .intsat_statistics
                        .intsat_constraint_avg_lhs_coeff
                        .add_term(
                            conflicting_constraint
                                .lhs
                                .iter()
                                .map(|(_, scale)| scale.abs())
                                .max()
                                .unwrap() as u64,
                        );

                    return Some(Constraint(LearnedConstraint {
                        constraint: conflicting_constraint,
                        backjump_level,
                    }));
                }
            }
        }
    }

    fn process(
        &mut self,
        context: &mut ConflictAnalysisContext,
        resolve_result: &Option<ConflictResolveResult>,
    ) -> Result<(), ()> {
        let resolve_result_unwrap = resolve_result
            .as_ref()
            .expect("Expected nogood / constraint");

        let Constraint(learned_constraint) = resolve_result_unwrap else {
            return self.resolution_resolver.process(context, resolve_result);
        };

        debug!(
            "==> Backtrack to {:?} (current = {:?})",
            learned_constraint.backjump_level,
            context.assignments.num_trail_entries() - 1
        );

        context.backtrack(learned_constraint.backjump_level);

        debug!(
            "==> Backtracked to {:?} (current = {:?})",
            context.assignments.get_decision_level(),
            context.assignments.num_trail_entries() - 1
        );

        let new_linear_prop = LinearLessOrEqualPropagator::new_learned(
            learned_constraint.constraint.to_vars().into_boxed_slice(),
            learned_constraint.constraint.rhs,
        );
        let new_propagator_id = context.propagators.alloc(Box::new(new_linear_prop), None);
        let new_propagator = &mut context.propagators[new_propagator_id];

        let mut initialisation_context = PropagatorInitialisationContext::new(
            &mut context.watch_list_cp,
            new_propagator_id,
            &context.assignments,
        );

        let _ = new_propagator.initialise_at_root(&mut initialisation_context);

        // We know this the previous call can result in Err (as we also backtrack when the
        // constraint is still conflicting) We do not return an error however, as the fact
        // that the propagator is not happy will be detected next cycle IntSat: re-start
        // conflict analysis in this case, we mimic this here
        context
            .propagator_queue
            .enqueue_propagator(new_propagator_id, new_propagator.priority());
        Ok(())
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

#[cfg(test)]
mod tests {
    use crate::engine::conflict_analysis::IntSatConflictResolver;
    use crate::engine::propagation::linear_less_or_equal::LinearLessOrEqual;
    use crate::variables::DomainId;

    fn construct_test_vars() -> [DomainId; 5] {
        let a = DomainId::new(0);
        let b = DomainId::new(1);
        let c = DomainId::new(2);
        let d = DomainId::new(3);
        let e = DomainId::new(4);

        [a, b, c, d, e]
    }

    #[test]
    fn test_cut_simple() {
        let [a, b, c, d, e] = construct_test_vars();

        let x = LinearLessOrEqual {
            lhs: vec![(a, 10), (b, 2), (c, 4), (d, -5)],
            rhs: 0,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, -4), (b, -4), (c, 6), (e, 8)],
            rhs: 0,
        };

        let cut_result = IntSatConflictResolver::apply_cut(a, &x, &y);
    }

    #[test]
    fn test_cut_same_coeff() {}

    #[test]
    fn test_cut_no_clash() {}

    #[test]
    fn test_cut_fully_clash() {}

    #[test]
    fn test_cut_fully_resolves() {}

    #[test]
    fn test_cut_contradiction() {}

    #[test]
    fn test_cut_nothing_learned() {}

    #[test]
    fn test_cut_overflow() {}

    #[test]
    fn test_cut_normalisation() {}
}
