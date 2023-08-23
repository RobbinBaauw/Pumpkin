//! A variable, in the context of the solver, is a view onto a domain. It may forward domain
//! information unaltered, or apply transformations which can be performed without the need of
//! constraints.

use crate::engine::{Change, Delta, DomainEvent, DomainManager, DomainOperationOutcome, Watchers};

use super::{DomainId, Predicate, PredicateConstructor};

pub trait IntVar: Clone + PredicateConstructor<Value = i32> {
    /// Get the lower bound of the variable.
    fn lower_bound(&self, domains: &DomainManager) -> i32;

    /// Get the upper bound of the variable.
    fn upper_bound(&self, domains: &DomainManager) -> i32;

    /// Determine whether the value is in the domain of this variable.
    fn contains(&self, domains: &DomainManager, value: i32) -> bool;

    /// Remove a value from the domain of this variable.
    fn remove(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome;

    /// Tighten the lower bound of the domain of this variable.
    fn set_lower_bound(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome;

    /// Tighten the upper bound of the domain of this variable.
    fn set_upper_bound(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome;

    /// Register a watch for this variable on the given domain event.
    fn watch(&self, watchers: &mut Watchers<'_>, event: DomainEvent);

    /// Decode a delta into the change it represents for this variable.
    fn unpack(&self, delta: Delta) -> Change;

    /// Get a variable which domain is scaled compared to the domain of self.
    ///
    /// The scaled domain will have holes in it. E.g. if we have `dom(x) = {1, 2}`, then
    /// `dom(x.scaled(2)) = {2, 4}` and *not* `dom(x.scaled(2)) = {1, 2, 3, 4}`.
    fn scaled(&self, scale: i32) -> AffineView<Self> {
        AffineView::new(self.clone(), scale, 0)
    }

    /// Get a variable which domain has a constant offset to the domain of self.
    fn offset(&self, offset: i32) -> AffineView<Self> {
        AffineView::new(self.clone(), 1, offset)
    }
}

/// Models the constraint `y = ax + b`, by expressing the domain of `y` as a transformation of the domain of `x`.
#[derive(Clone)]
pub struct AffineView<View> {
    inner: View,
    scale: i32,
    offset: i32,
}

impl IntVar for DomainId {
    fn lower_bound(&self, domains: &DomainManager) -> i32 {
        domains.get_lower_bound(*self)
    }

    fn upper_bound(&self, domains: &DomainManager) -> i32 {
        domains.get_upper_bound(*self)
    }

    fn contains(&self, domains: &DomainManager, value: i32) -> bool {
        domains.is_value_in_domain(*self, value)
    }

    fn remove(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome {
        domains.remove_value_from_domain(*self, value)
    }

    fn set_lower_bound(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome {
        domains.tighten_lower_bound(*self, value)
    }

    fn set_upper_bound(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome {
        domains.tighten_upper_bound(*self, value)
    }

    fn watch(&self, watchers: &mut Watchers<'_>, event: DomainEvent) {
        watchers.watch(*self, event);
    }

    fn unpack(&self, delta: Delta) -> Change {
        delta.unwrap_change()
    }
}

impl<View> AffineView<View> {
    pub fn new(inner: View, scale: i32, offset: i32) -> Self {
        AffineView {
            inner,
            scale,
            offset,
        }
    }

    fn invert(&self, value: i32) -> Option<i32> {
        let inverted_translation = value - self.offset;

        if inverted_translation % self.scale == 0 {
            Some(inverted_translation / self.scale)
        } else {
            None
        }
    }

    fn map(&self, value: i32) -> i32 {
        self.scale * value + self.offset
    }
}

impl<View> IntVar for AffineView<View>
where
    View: IntVar,
{
    fn lower_bound(&self, domains: &DomainManager) -> i32 {
        self.map(self.inner.lower_bound(domains))
    }

    fn upper_bound(&self, domains: &DomainManager) -> i32 {
        self.map(self.inner.upper_bound(domains))
    }

    fn contains(&self, domains: &DomainManager, value: i32) -> bool {
        self.invert(value)
            .map(|v| self.inner.contains(domains, v))
            .unwrap_or(false)
    }

    fn remove(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome {
        self.invert(value)
            .map(|v| self.inner.remove(domains, v))
            .unwrap_or(DomainOperationOutcome::Failure)
    }

    fn set_lower_bound(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome {
        self.invert(value)
            .map(|v| self.inner.set_lower_bound(domains, v))
            .unwrap_or(DomainOperationOutcome::Failure)
    }

    fn set_upper_bound(&self, domains: &mut DomainManager, value: i32) -> DomainOperationOutcome {
        self.invert(value)
            .map(|v| self.inner.set_upper_bound(domains, v))
            .unwrap_or(DomainOperationOutcome::Failure)
    }

    fn watch(&self, watchers: &mut Watchers<'_>, event: DomainEvent) {
        if self.scale.is_negative() {
            match event {
                DomainEvent::LowerBound => self.inner.watch(watchers, DomainEvent::UpperBound),
                DomainEvent::UpperBound => self.inner.watch(watchers, DomainEvent::LowerBound),
                event => self.inner.watch(watchers, event),
            }
        } else {
            self.inner.watch(watchers, event);
        }
    }

    fn unpack(&self, delta: Delta) -> Change {
        match self.inner.unpack(delta) {
            Change::Removal(value) => Change::Removal(self.map(value)),
            Change::LowerBound(_) => todo!(),
            Change::UpperBound(_) => todo!(),
        }
    }
}

impl<Var: PredicateConstructor> PredicateConstructor for AffineView<Var> {
    type Value = Var::Value;

    fn lower_bound_predicate(&self, _bound: Self::Value) -> Predicate {
        todo!("Handle case where invert() returns None.")
    }

    fn upper_bound_predicate(&self, _bound: Self::Value) -> Predicate {
        todo!("Handle case where invert() returns None.")
    }

    fn equality_predicate(&self, _bound: Self::Value) -> Predicate {
        todo!("Handle case where invert() returns None.")
    }

    fn disequality_predicate(&self, _bound: Self::Value) -> Predicate {
        todo!("Handle case where invert() returns None.")
    }
}
