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
use crate::engine::Assignments;
use crate::engine::DomainEvents;
use crate::predicates::Predicate;
use crate::predicates::PropositionalConjunction;
use crate::variables::DomainId;
use crate::variables::IntegerVariable;

#[derive(Clone, Debug)]
pub(crate) struct PredicateLiteralPropagator {
    predicate: Predicate,
    predicate_var_id: LocalId,
    literal: DomainId,
    literal_id: LocalId,
}

impl PredicateLiteralPropagator {
    pub(crate) fn new(predicate: Predicate, literal: DomainId) -> Self {
        PredicateLiteralPropagator {
            predicate,
            predicate_var_id: LocalId::from(0),
            literal,
            literal_id: LocalId::from(0),
        }
    }

    fn get_propagation_reason_constraint(&self, assignments: &Assignments) -> LinearLessOrEqual {
        let pred_var_id = self.predicate.get_domain();
        let init_lb = assignments.get_initial_lower_bound(pred_var_id);
        let init_ub = assignments.get_initial_upper_bound(pred_var_id);

        match self.predicate {
            Predicate::LowerBound { lower_bound, .. } => {
                let big_m_lb = (-init_lb + lower_bound).max(0);
                let big_m_ub = (init_ub - lower_bound + 1).max(0);
                let big_m = big_m_lb.max(big_m_ub);

                // -x + Mp <= -lb + M
                let opt_1 = LinearLessOrEqual {
                    lhs: vec![(pred_var_id, -1), (self.literal, big_m)],
                    rhs: -lower_bound + big_m,
                };

                // x - Mp <= lb - 1
                let opt_2 = LinearLessOrEqual {
                    lhs: vec![(pred_var_id, 1), (self.literal, -big_m)],
                    rhs: lower_bound - 1,
                };

                opt_1
            }
            Predicate::UpperBound { upper_bound, .. } => {
                let big_m_ub = (init_ub - upper_bound).max(0);
                let big_m_lb = (-init_lb + upper_bound + 1).max(0);
                let big_m = big_m_lb.max(big_m_ub);

                // x + Mp <= ub + M
                let opt_1 = LinearLessOrEqual {
                    lhs: vec![(pred_var_id, 1), (self.literal, big_m)],
                    rhs: upper_bound + big_m,
                };

                // -x - Mp <= -ub - 1
                let opt_2 = LinearLessOrEqual {
                    lhs: vec![(pred_var_id, -1), (self.literal, -big_m)],
                    rhs: -upper_bound - 1,
                };

                opt_1
            }
            Predicate::NotEqual { .. } | Predicate::Equal { .. } => {
                todo!("NotEqual and Equal predicates are not yet supported!")
            }
        }
    }
}

impl Propagator for PredicateLiteralPropagator {
    fn name(&self) -> &str {
        "PredicateLiteralPropagator"
    }

    fn debug_propagate_from_scratch(
        &self,
        mut context: PropagationContextMut,
    ) -> PropagationStatusCP {
        match context.assignments.evaluate_predicate(self.predicate) {
            Some(true) => context.set_lower_bound(
                &self.literal,
                1,
                (
                    PropositionalConjunction::from(self.predicate),
                    self.get_propagation_reason_constraint(context.assignments),
                ),
            )?,
            Some(false) => context.set_upper_bound(
                &self.literal,
                0,
                (
                    PropositionalConjunction::from(!self.predicate),
                    self.get_propagation_reason_constraint(context.assignments),
                ),
            )?,
            None => {}
        };

        let lit_lb = self.literal.lower_bound(context.assignments);
        if lit_lb >= 1 {
            context.post_predicate(
                self.predicate,
                (
                    conjunction!([self.literal >= 1]),
                    self.get_propagation_reason_constraint(context.assignments),
                ),
            )?;
        }

        let lit_ub = self.literal.upper_bound(context.assignments);
        if lit_ub <= 0 {
            context.post_predicate(
                !self.predicate,
                (
                    conjunction!([self.literal <= 0]),
                    self.get_propagation_reason_constraint(context.assignments),
                ),
            )?;
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
        self.predicate_var_id = context.get_next_local_id();
        let _ = context.register_unchecked(
            self.predicate.get_domain(),
            DomainEvents::ANY_INT,
            self.predicate_var_id,
        );
        let _ = context.register_for_backtrack_events(
            self.predicate.get_domain(),
            DomainEvents::ANY_INT,
            self.predicate_var_id,
        );

        self.literal_id = context.get_next_local_id();
        let _ = context.register_unchecked(self.literal, DomainEvents::ANY_INT, self.literal_id);
        let _ = context.register_for_backtrack_events(
            self.literal,
            DomainEvents::ANY_INT,
            self.literal_id,
        );

        Ok(())
    }
}
