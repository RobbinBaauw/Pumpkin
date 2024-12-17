use itertools::Itertools;
use log::debug;

use crate::basic_types::linear_less_or_equal::LinearLessOrEqual;
use crate::basic_types::moving_averages::MovingAverage;
use crate::basic_types::StoredConflictInfo;
use crate::engine::conflict_analysis::ConflictAnalysisContext;
use crate::engine::conflict_analysis::ConflictResolveResult;
use crate::engine::conflict_analysis::ConflictResolveResult::Constraint;
use crate::engine::conflict_analysis::ConflictResolveResult::Nogood;
use crate::engine::conflict_analysis::ConflictResolver;
use crate::engine::conflict_analysis::LearnedConstraint;
use crate::engine::conflict_analysis::LearnedNogood;
use crate::engine::propagation::{Propagator, PropagatorId, PropagatorInitialisationContext};
use crate::engine::ResolutionResolver;
use crate::predicates::Predicate;
use crate::propagators::linear_less_or_equal::LinearLessOrEqualPropagator;
use crate::propagators::predicate_literal_propagator::PredicateLiteralPropagator;
use crate::pumpkin_assert_ne_simple;
use crate::pumpkin_assert_simple;
use crate::statistics::learned_constraint_log::LearnedConstraintLogItem;
use crate::statistics::statistic_logger::Propagator;
use crate::variables::{DomainId, Literal};

static LOG_NOGOODS: bool = false;

#[derive(Debug, Default)]
pub(crate) struct IntSatConflictResolver {
    resolution_resolver: ResolutionResolver,
}

#[derive(Debug, Default)]
struct CutSuccess {
    inequality: LinearLessOrEqual,
    skip_early_backjump: bool,
}

#[derive(Debug)]
enum CutError {
    NothingLearned,
    Overflow,
    Contradiction,
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

    fn create_new_propagator(context: &mut ConflictAnalysisContext, propagator: impl Propagator) -> PropagatorId {
        let new_pred_prop_id = context.propagators.alloc(Box::new(propagator), None);
        let new_propagator = &mut context.propagators[new_pred_prop_id];

        let mut initialisation_context = PropagatorInitialisationContext::new(
            &mut context.watch_list_cp,
            new_pred_prop_id,
            &context.assignments,
        );

        let _ = new_propagator.initialise_at_non_root(&mut initialisation_context);

        context
            .propagator_queue
            .enqueue_propagator(new_pred_prop_id, new_propagator.priority());

        new_pred_prop_id
    }

    fn apply_cut(
        var: DomainId,
        c1: &LinearLessOrEqual,
        c2: &LinearLessOrEqual,
    ) -> Result<CutSuccess, CutError> {
        let c1_scale = c1.find_variable_scale(var).unwrap();
        let c2_scale = c2.find_variable_scale(var).unwrap();

        // A pre-condition to apply a cut is that both constraints have 'var'
        // and that they have opposite signs
        pumpkin_assert_ne_simple!(c1_scale.is_positive(), c2_scale.is_positive());

        let g = gcd(c1_scale.abs(), c2_scale.abs());
        let mult_c1 = c2_scale.abs() / g;
        let mult_c2 = c1_scale.abs() / g;

        let mut skip_early_backjump = true;

        let mut c1_sorted = c1.lhs.iter().sorted_by_key(|(id, _)| id.id).peekable();
        let mut c2_sorted = c2.lhs.iter().sorted_by_key(|(id, _)| id.id).peekable();

        let mut new_lhs: Vec<(DomainId, i32)> = vec![];

        let mult_or_err = |item: Option<&&(DomainId, i32)>, mult: i32| {
            item.map(|(id, curr_scale)| {
                let new_scale = mult.checked_mul(*curr_scale).ok_or(CutError::Overflow)?;
                Ok((*id, new_scale))
            })
            .transpose()
        };

        while c1_sorted.peek().is_some() || c2_sorted.peek().is_some() {
            let c1_item = mult_or_err(c1_sorted.peek(), mult_c1)?;
            let c2_item = mult_or_err(c2_sorted.peek(), mult_c2)?;

            match (c1_item, c2_item) {
                (Some((c1_id, c1_scale)), Some((c2_id, _))) if c1_id.id < c2_id.id => {
                    new_lhs.push((c1_id, c1_scale));
                    let _ = c1_sorted.next();
                }
                (Some((c1_id, _)), Some((c2_id, c2_scale))) if c2_id.id < c1_id.id => {
                    new_lhs.push((c2_id, c2_scale));
                    let _ = c2_sorted.next();
                }
                (Some((c1_id, c1_scale)), Some((c2_id, c2_scale))) if c1_id.id == c2_id.id => {
                    // Don't skip early backjump in case there is a clash between variables that
                    // are not 'var'
                    if c1_id != var {
                        skip_early_backjump = false;
                    }

                    let new_scale = c1_scale.checked_add(c2_scale).ok_or(CutError::Overflow)?;
                    if new_scale != 0 {
                        new_lhs.push((c1_id, new_scale));
                    }

                    let _ = c1_sorted.next();
                    let _ = c2_sorted.next();
                }
                (Some((c1_id, c1_scale)), None) => {
                    new_lhs.push((c1_id, c1_scale));
                    let _ = c1_sorted.next();
                }
                (None, Some((c2_id, c2_scale))) => {
                    new_lhs.push((c2_id, c2_scale));
                    let _ = c2_sorted.next();
                }
                _ => unreachable!("Shouldn't be possible"),
            }
        }

        pumpkin_assert_simple!(
            !new_lhs.iter().any(|(k, _)| *k == var),
            "variable not eliminated"
        );

        let c1_rhs_scaled = c1.rhs.checked_mul(mult_c1).ok_or(CutError::Overflow)?;
        let c2_rhs_scaled = c2.rhs.checked_mul(mult_c2).ok_or(CutError::Overflow)?;
        let mut new_rhs = c1_rhs_scaled
            .checked_add(c2_rhs_scaled)
            .ok_or(CutError::Overflow)?;

        if new_lhs.len() == 0 {
            return Err(if new_rhs < 0 {
                CutError::Contradiction
            } else {
                CutError::NothingLearned
            });
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

        Ok(CutSuccess {
            inequality: LinearLessOrEqual {
                lhs: new_lhs,
                rhs: new_rhs,
            },
            skip_early_backjump,
        })
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
            StoredConflictInfo::RootLevelConflict(..) => {
                unreachable!("Shouldn't have to explain a root level conflict")
            }
        };

        let current_decision_level = context.assignments.get_decision_level();
        let mut trail_index = context.assignments.num_trail_entries() - 1;

        loop {
            debug!("========");
            debug!("Conflicting constraint: {conflicting_constraint}");

            // Find trail entry at which the conflicting constraint is not conflicting anymore
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

            // TODO: this is flipped compared to IntSat's behavior. But this is how I performed all
            // previous experiments, and quick tests show that flipping this is NOT a good
            // condition. Check what the merit of this was and whether we need to still
            // use it. This goes a bit in tandem with the other is_conflicting
            // check later. I can see the reason for these checks; making sure we
            // do not continue a search that is unlikely to lead us to an asserting learned
            // constraint, but on the other hand, you are throwing away some
            // possibly useful combinations. For now, I leave it as it was during the experiments.

            // Once we have found a conflicting trail level, use this level to start our
            // analysis
            if !conflicting_constraint.is_conflicting(context.assignments, trail_index) {
                trail_index -= 1;
                continue;
            }

            debug!("==>==> Using {trail_index}");
            trail_index -= 1;

            // Find the scale of the variable of its reason
            let propagator_id = context
                .reason_store
                .get_propagator(trail_entry.reason.unwrap());
            let propagator = &context.propagators[propagator_id];

            let prop_constraint_expl_opt = propagator.linear_inequality_explanation();
            let Some(prop_constraint_expl) = prop_constraint_expl_opt else {
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

                // For now, we fall back to resolution and this is likely out of scope.
                return self.apply_fallback(context, "Detected nogoods");
            };

            debug!(
                "==>==> Merging with {:?}: {prop_constraint_expl}",
                trail_entry.predicate.get_domain()
            );

            // Because a lineq propagator propagates multiple upper bounds at the same time, the
            // last one might not be the one actually causing the conflict. The one that caused
            // the conflict should have a different sign. We search until we find that one.
            let cutting_var = trail_entry.predicate.get_domain();
            let c1_scale = conflicting_constraint
                .find_variable_scale(cutting_var)
                .unwrap();
            let c2_scale = prop_constraint_expl
                .find_variable_scale(cutting_var)
                .unwrap();

            if c1_scale.is_positive() == c2_scale.is_positive() {
                debug!("==> Not different signs, retry");
                continue;
            }

            let (new_conflicting_constraint, skip_early_backjump) = match Self::apply_cut(
                cutting_var,
                &conflicting_constraint,
                &prop_constraint_expl,
            ) {
                Err(CutError::NothingLearned) => {
                    return self.apply_fallback(context, "Nothing learned");
                }
                Err(CutError::Overflow) => {
                    return self.apply_fallback(context, "Overflow");
                }
                Err(CutError::Contradiction) => {
                    debug!("==>==> Contradiction, unsat!");
                    return Some(Nogood(LearnedNogood {
                        predicates: vec![Predicate::trivially_true()],
                        alternative_constraint: None,
                        backjump_level: 0,
                    }));
                }
                Ok(CutSuccess {
                    inequality: constraint,
                    skip_early_backjump,
                }) => (constraint, skip_early_backjump),
            };

            debug!("==> New conflicting constraint after eliminating {:?}: {new_conflicting_constraint}", trail_entry.predicate.get_domain());

            // Check whether the newly learned conflicting constraint overflows with the current
            // assignments
            if new_conflicting_constraint.overflows(context.assignments, trail_index) {
                return self.apply_fallback(context, "Overflow");
            }

            // TODO: check whether this condition helps (and what trail_index would make sense), see
            // explanation on previous condition

            // If this new constraint is not false at the current height, we skip it and apply
            // resolution
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

                // Check whether the newly learned conflicting constraint overflows with the
                // assignments at that level
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
                    let res = self.resolution_resolver.resolve_conflict(context);
                    let Some(Nogood(mut learned_nogood)) = res else {
                        unreachable!("resolution should always learn something")
                    };

                    if let Some(learned_constraint_log) = &mut context.learned_constraint_log {
                        let learned_constraint = conflicting_constraint.clone();

                        learned_constraint_log.log_item(LearnedConstraintLogItem::ConflictResult {
                            learned_constraint,
                            learned_nogoods: learned_nogood.predicates.clone().into(),
                        });
                    }

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

                    return if LOG_NOGOODS {
                        learned_nogood.alternative_constraint = Some(conflicting_constraint);
                        Some(Nogood(learned_nogood))
                    } else {
                        Some(Constraint(LearnedConstraint {
                            constraint: conflicting_constraint,
                            alternative_nogood: learned_nogood.predicates,
                            backjump_level,
                        }))
                    }
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

        let Constraint(mut learned_constraint) = resolve_result_unwrap else {
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

        // - Create binary variables & propagator
        // - (either) Set the lower/upper bound updates to reflect the correct state (with what trail entries?)
        //   (or) Alternatively, dynamically retrieve the state (though then there are still no trail entries)
        // - Always trigger the propagators for the learned constraints on backtrack!

        // We actually never need to put anything on the trail. Just make sure it propagates on backtracking...
        // The LinLeq propagator only requires lower bounds at this point in time,
        // which is always correct if you immediately apply propagator after backtracking.
        // Same for nogood propagator. Let's see...
        for (var_id, var_pred) in learned_constraint.auxiliary_variables {
            let new_var_id = context.assignments.new_aux_variable();

            // Update the ids using this var to their new ids
            learned_constraint.constraint
                .lhs
                .iter_mut()
                .filter(|(id, scale)| *id == var_id)
                .for_each(|(id, scale)| *id = new_var_id);

            let new_pred_prop = PredicateLiteralPropagator::new(var_pred, new_var_id);
            let _ = Self::create_new_propagator(context, new_pred_prop);
        }

        let new_linear_prop = LinearLessOrEqualPropagator::new_learned(
            learned_constraint.constraint.to_vars().into_boxed_slice(),
            learned_constraint.constraint.rhs,
            context.assignments,
            learned_constraint.alternative_nogood.clone(),
        );
        let new_propagator_id = Self::create_new_propagator(context, new_linear_prop);

        if !LOG_NOGOODS {
            if let Some(learned_constraint_log) = &mut context.learned_constraint_log {
                learned_constraint_log.log_item(LearnedConstraintLogItem::NewPropagator {
                    propagator_id: new_propagator_id.0,
                    learned_constraint: learned_constraint.constraint.clone(),
                });
            }
        }

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

// Taken from https://docs.rs/num-integer/latest/src/num_integer/lib.rs.html#420-422
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
    use crate::basic_types::linear_less_or_equal::LinearLessOrEqual;
    use crate::engine::conflict_analysis::resolvers::intsat_conflict_resolver::CutError::Contradiction;
    use crate::engine::conflict_analysis::resolvers::intsat_conflict_resolver::CutError::NothingLearned;
    use crate::engine::conflict_analysis::resolvers::intsat_conflict_resolver::CutError::Overflow;
    use crate::engine::conflict_analysis::resolvers::intsat_conflict_resolver::CutSuccess;
    use crate::engine::conflict_analysis::IntSatConflictResolver;
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
            rhs: 12,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, -4), (b, -4), (c, 6), (e, 8)],
            rhs: -6,
        };

        let CutSuccess {
            skip_early_backjump,
            inequality,
        } = IntSatConflictResolver::apply_cut(a, &x, &y).unwrap();

        let z = LinearLessOrEqual {
            lhs: vec![(b, -8), (c, 19), (d, -5), (e, 20)],
            rhs: -3,
        };

        assert_eq!(
            skip_early_backjump, false,
            "should be backjumping early due to clash"
        );
        assert_eq!(inequality, z);
    }

    #[test]
    fn test_cut_same_coeff() {
        let [a, b, c, d, e] = construct_test_vars();

        let x = LinearLessOrEqual {
            lhs: vec![(a, -4), (b, 2), (c, 4), (d, -5)],
            rhs: 10,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, 4), (b, -4), (c, 6), (e, 8)],
            rhs: -5,
        };

        let CutSuccess {
            skip_early_backjump,
            inequality,
        } = IntSatConflictResolver::apply_cut(a, &x, &y).unwrap();

        let z = LinearLessOrEqual {
            lhs: vec![(b, -2), (c, 10), (d, -5), (e, 8)],
            rhs: 5,
        };

        assert_eq!(
            skip_early_backjump, false,
            "should be backjumping early due to clash"
        );
        assert_eq!(inequality, z);
    }

    #[test]
    fn test_cut_no_clash() {
        let [a, b, c, d, e] = construct_test_vars();

        let x = LinearLessOrEqual {
            lhs: vec![(a, -4), (b, 2), (c, 4), (d, -5)],
            rhs: 10,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, 4), (e, 8)],
            rhs: -5,
        };

        let CutSuccess {
            skip_early_backjump,
            inequality,
        } = IntSatConflictResolver::apply_cut(a, &x, &y).unwrap();

        let z = LinearLessOrEqual {
            lhs: vec![(b, 2), (c, 4), (d, -5), (e, 8)],
            rhs: 5,
        };

        assert_eq!(
            skip_early_backjump, true,
            "should not be backjumping early due to lack of clash"
        );
        assert_eq!(inequality, z);
    }

    #[test]
    fn test_cut_fully_clash() {
        let [a, b, c, d, e] = construct_test_vars();

        let x = LinearLessOrEqual {
            lhs: vec![(a, -10), (b, 2), (c, 4), (d, -5), (e, -1)],
            rhs: 10,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, 4), (b, -4), (c, 6), (d, 5), (e, 8)],
            rhs: -5,
        };

        let CutSuccess {
            skip_early_backjump,
            inequality,
        } = IntSatConflictResolver::apply_cut(a, &x, &y).unwrap();

        let z = LinearLessOrEqual {
            lhs: vec![(b, -16), (c, 38), (d, 15), (e, 38)],
            rhs: -5,
        };

        assert_eq!(
            skip_early_backjump, false,
            "should be backjumping early due to clash"
        );
        assert_eq!(inequality, z);
    }

    #[test]
    fn test_cut_contradiction() {
        let [a, b, c, d, e] = construct_test_vars();

        let x = LinearLessOrEqual {
            lhs: vec![(a, -2), (b, 2), (c, -3), (d, -5), (e, -4)],
            rhs: 0,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, 4), (b, -4), (c, 6), (d, 10), (e, 8)],
            rhs: -5,
        };

        let cut_result = IntSatConflictResolver::apply_cut(a, &x, &y).unwrap_err();

        assert!(matches!(cut_result, Contradiction {}));
    }

    #[test]
    fn test_cut_nothing_learned() {
        let [a, b, c, d, e] = construct_test_vars();

        let x = LinearLessOrEqual {
            lhs: vec![(a, -2), (b, 2), (c, -3), (d, -5), (e, -4)],
            rhs: 10,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, 4), (b, -4), (c, 6), (d, 10), (e, 8)],
            rhs: -5,
        };

        let cut_result = IntSatConflictResolver::apply_cut(a, &x, &y).unwrap_err();

        assert!(matches!(cut_result, NothingLearned {}));
    }

    #[test]
    fn test_cut_overflow() {
        let [a, b, c, d, e] = construct_test_vars();

        let x = LinearLessOrEqual {
            lhs: vec![(a, -99997), (b, 223545223), (c, -3), (d, -5), (e, -4)],
            rhs: 10,
        };

        let y = LinearLessOrEqual {
            lhs: vec![(a, 99995), (b, -1000), (c, 6), (d, 10), (e, 8)],
            rhs: -5,
        };

        let cut_result = IntSatConflictResolver::apply_cut(a, &x, &y).unwrap_err();

        assert!(matches!(cut_result, Overflow {}));
    }
}
