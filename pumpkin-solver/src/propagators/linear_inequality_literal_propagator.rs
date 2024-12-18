use crate::basic_types::linear_less_or_equal::LinearLessOrEqual;
use crate::basic_types::PropagationReason;
use crate::basic_types::PropagationStatusCP;
use crate::conjunction;
use crate::engine::opaque_domain_event::OpaqueDomainEvent;
use crate::engine::propagation::EnqueueDecision;
use crate::engine::propagation::LocalId;
use crate::engine::propagation::PropagationContext;
use crate::engine::propagation::PropagationContextMut;
use crate::engine::propagation::Propagator;
use crate::engine::propagation::PropagatorInitialisationContext;
use crate::engine::propagation::ReadDomains;
use crate::engine::Assignments;
use crate::engine::DomainEvents;
use crate::predicate;
use crate::predicates::PropositionalConjunction;
use crate::variables::DomainId;
use crate::variables::IntegerVariable;

#[derive(Clone, Debug)]
pub(crate) struct LinearInequalityLiteralPropagator {
    linear_inequality: LinearLessOrEqual,
    literal: DomainId,
}

impl LinearInequalityLiteralPropagator {
    pub(crate) fn new(linear_inequality: LinearLessOrEqual, literal: DomainId) -> Self {
        LinearInequalityLiteralPropagator {
            linear_inequality,
            literal,
        }
    }

    fn get_propagation_reason_constraint(
        &self,
        assignments: &Assignments,
        trail_position: usize,
    ) -> LinearLessOrEqual {
        // We're linking Ax <= b <-> p, meaning we have two equations:
        // * Ax <= b + M(1-p)
        // * Ax > b - Mp, or Ax >= b + 1 - Mp, or -Ax <= -b - 1 + Mp
        //
        // We need M to be sufficiently large, and then as small as possible.
        // Assume the same equations in which M has to take effect:
        // * Ax - b <= M
        // * -Ax + b + 1 <= M
        //
        // We can find the value for M by finding the maximal value of the LHS now (using initial
        // domains):
        // * ub(Ax) - b <= M
        // * -lb(Ax) + b + 1 <= M
        //
        // We take the maximum of both found M's to find the final M.
        // If M is negative, we do not need it, so we have found a global constraint and can just
        // set M to 0
        //
        // After finding M, we again take the first versions of the equations and map these into new
        // linear inequalities

        let lb_lhs = self.linear_inequality.lb_lhs(assignments, trail_position) as i32; // TODO handle overflows
        let ub_lhs = self.linear_inequality.ub_lhs(assignments, trail_position) as i32; // TODO handle overflows
        let rhs = self.linear_inequality.rhs;

        let big_m_opt_1 = (ub_lhs - rhs).max(0);
        let big_m_opt_2 = (-lb_lhs + rhs + 1).max(0);
        let big_m = big_m_opt_1.max(big_m_opt_2);

        // Ax + Mp <= b + M
        let mut opt_1_lhs = self.linear_inequality.lhs.clone();
        opt_1_lhs.push((self.literal, big_m));

        let opt_1 = LinearLessOrEqual {
            lhs: opt_1_lhs,
            rhs: rhs + big_m,
        };

        // -Ax - Mp <= -b - 1
        let mut opt_2_lhs = self.linear_inequality.lhs.clone();
        opt_2_lhs.iter_mut().for_each(|(_, scale)| *scale *= -1);
        opt_2_lhs.push((self.literal, -big_m));

        let opt_2 = LinearLessOrEqual {
            lhs: opt_2_lhs,
            rhs: -rhs - 1,
        };

        opt_1 // TODO pick the best option
    }
}

impl Propagator for LinearInequalityLiteralPropagator {
    fn name(&self) -> &str {
        "PredicateLiteralPropagator"
    }

    fn debug_propagate_from_scratch(
        &self,
        mut context: PropagationContextMut,
    ) -> PropagationStatusCP {
        let trail_position = context.assignments.num_trail_entries() - 1;

        match self
            .linear_inequality
            .evaluate_at_trail_position(context.assignments, trail_position)
        {
            Some(true) => {
                let conjunction: PropositionalConjunction = self
                    .linear_inequality
                    .lhs
                    .iter()
                    .map(|(var_id, _)| {
                        predicate![
                            var_id <= context.upper_bound_at_trail_position(var_id, trail_position)
                        ]
                    })
                    .collect();

                context.set_lower_bound(
                    &self.literal,
                    1,
                    (
                        conjunction,
                        self.get_propagation_reason_constraint(context.assignments, trail_position),
                    ),
                )?
            }
            Some(false) => {
                let conjunction: PropositionalConjunction = self
                    .linear_inequality
                    .lhs
                    .iter()
                    .map(|(var_id, _)| {
                        predicate![
                            var_id >= context.lower_bound_at_trail_position(var_id, trail_position)
                        ]
                    })
                    .collect();

                context.set_upper_bound(
                    &self.literal,
                    0,
                    (
                        conjunction,
                        self.get_propagation_reason_constraint(context.assignments, trail_position),
                    ),
                )?
            }
            None => {}
        };

        // Predicate that can be propagated!
        if self.linear_inequality.lhs.len() == 1
            && context.assignments.is_domain_assigned(&self.literal)
        {
            let (var_id, var_scale) = self.linear_inequality.lhs[0];
            let pred = predicate![var_id <= self.linear_inequality.rhs / var_scale];

            let lit_lb = self.literal.lower_bound(context.assignments);
            if lit_lb == 1 {
                context.post_predicate(
                    pred,
                    (
                        conjunction!([self.literal >= 1]),
                        self.get_propagation_reason_constraint(context.assignments, trail_position),
                    ),
                )?;
            } else {
                context.post_predicate(
                    !pred,
                    (
                        conjunction!([self.literal <= 0]),
                        self.get_propagation_reason_constraint(context.assignments, trail_position),
                    ),
                )?;
            }
        }

        Ok(())
    }

    fn notify_backtrack(
        &mut self,
        _: PropagationContext,
        _: LocalId,
        _: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        EnqueueDecision::Enqueue
    }

    fn priority(&self) -> u32 {
        0
    }

    fn initialise_at_root(
        &mut self,
        context: &mut PropagatorInitialisationContext,
    ) -> Result<(), PropagationReason> {
        self.linear_inequality.lhs.iter().for_each(|(var_id, _)| {
            let local_var_id = context.get_next_local_id();
            let _ = context.register_unchecked(*var_id, DomainEvents::ANY_INT, local_var_id);

            let _ =
                context.register_for_backtrack_events(*var_id, DomainEvents::ANY_INT, local_var_id);
        });

        let literal_id = context.get_next_local_id();
        let _ = context.register_unchecked(self.literal, DomainEvents::ANY_INT, literal_id);
        let _ =
            context.register_for_backtrack_events(self.literal, DomainEvents::ANY_INT, literal_id);

        Ok(())
    }
}
