use crate::basic_types::PropagationReason;
use crate::basic_types::PropagationStatusCP;
use crate::engine::opaque_domain_event::OpaqueDomainEvent;
use crate::engine::propagation::EnqueueDecision;
use crate::engine::propagation::LocalId;
use crate::engine::propagation::PropagationContext;
use crate::engine::propagation::PropagationContextMut;
use crate::engine::propagation::Propagator;
use crate::engine::propagation::PropagatorInitialisationContext;
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
}

impl Propagator for PredicateLiteralPropagator {
    fn name(&self) -> &str {
        "PredicateLiteralPropagator"
    }

    fn debug_propagate_from_scratch(
        &self,
        mut context: PropagationContextMut,
    ) -> PropagationStatusCP {
        // TODO proper reasons for propagations
        match context.assignments.evaluate_predicate(self.predicate) {
            Some(true) => self.literal.set_lower_bound(context.assignments, 1, None)?,
            Some(false) => self.literal.set_upper_bound(context.assignments, 0, None)?,
            None => {}
        };

        let lit_lb = self.literal.lower_bound(context.assignments);
        if lit_lb >= 1 {
            context.post_predicate(self.predicate, PropositionalConjunction::new(vec![]))?;
        }

        let lit_ub = self.literal.upper_bound(context.assignments);
        if lit_ub <= 0 {
            context.post_predicate(!self.predicate, PropositionalConjunction::new(vec![]))?;
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
