//! A variable, in the context of the solver, is a view onto a domain. It may forward domain
//! information unaltered, or apply transformations which can be performed without the need of
//! constraints.

use std::cmp::Ordering;

use enumset::EnumSet;

use crate::basic_types::DomainId;
use crate::basic_types::Predicate;
use crate::basic_types::PredicateConstructor;
use crate::engine::reason::ReasonRef;
use crate::engine::AssignmentsInteger;
use crate::engine::EmptyDomain;
use crate::engine::IntDomainEvent;
use crate::engine::OpaqueDomainEvent;
use crate::engine::Watchers;

pub trait IntVar: Clone + PredicateConstructor<Value = i32> {
    type AffineView: IntVar;

    /// Get the lower bound of the variable.
    fn lower_bound(&self, assignment: &AssignmentsInteger) -> i32;

    /// Get the upper bound of the variable.
    fn upper_bound(&self, assignment: &AssignmentsInteger) -> i32;

    /// Determine whether the value is in the domain of this variable.
    fn contains(&self, assignment: &AssignmentsInteger, value: i32) -> bool;

    /// Get a predicate description (bounds + holes) of the domain of this variable.
    /// N.B. can be very expensive with large domains, and very large with holey domains
    ///
    /// This should not be used to explicitly check for holes in the domain, but only to build
    /// explanations. If views change the observed domain, they will not change this description,
    /// because it should be a description of the domain in the solver.
    fn describe_domain(&self, assignment: &AssignmentsInteger) -> Vec<Predicate>;

    /// Remove a value from the domain of this variable.
    fn remove(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain>;

    /// Tighten the lower bound of the domain of this variable.
    fn set_lower_bound(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain>;

    /// Tighten the upper bound of the domain of this variable.
    fn set_upper_bound(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain>;

    /// Register a watch for this variable on the given domain events.
    fn watch_all(&self, watchers: &mut Watchers<'_>, events: EnumSet<IntDomainEvent>);

    /// Decode a domain event for this variable.
    fn unpack_event(&self, event: OpaqueDomainEvent) -> IntDomainEvent;

    /// Get a variable which domain is scaled compared to the domain of self.
    ///
    /// The scaled domain will have holes in it. E.g. if we have `dom(x) = {1, 2}`, then
    /// `dom(x.scaled(2)) = {2, 4}` and *not* `dom(x.scaled(2)) = {1, 2, 3, 4}`.
    fn scaled(&self, scale: i32) -> Self::AffineView;

    /// Get a variable which domain has a constant offset to the domain of self.
    fn offset(&self, offset: i32) -> Self::AffineView;
}

/// Models the constraint `y = ax + b`, by expressing the domain of `y` as a transformation of the
/// domain of `x`.
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct AffineView<Inner> {
    inner: Inner,
    scale: i32,
    offset: i32,
}

impl IntVar for DomainId {
    type AffineView = AffineView<Self>;

    fn lower_bound(&self, assignment: &AssignmentsInteger) -> i32 {
        assignment.get_lower_bound(*self)
    }

    fn upper_bound(&self, assignment: &AssignmentsInteger) -> i32 {
        assignment.get_upper_bound(*self)
    }

    fn contains(&self, assignment: &AssignmentsInteger, value: i32) -> bool {
        assignment.is_value_in_domain(*self, value)
    }

    fn describe_domain(&self, assignment: &AssignmentsInteger) -> Vec<Predicate> {
        assignment.get_domain_description(*self)
    }

    fn remove(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain> {
        assignment.remove_value_from_domain(*self, value, reason)
    }

    fn set_lower_bound(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain> {
        assignment.tighten_lower_bound(*self, value, reason)
    }

    fn set_upper_bound(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain> {
        assignment.tighten_upper_bound(*self, value, reason)
    }

    fn watch_all(&self, watchers: &mut Watchers<'_>, events: EnumSet<IntDomainEvent>) {
        watchers.watch_all(*self, events);
    }

    fn unpack_event(&self, event: OpaqueDomainEvent) -> IntDomainEvent {
        event.unwrap()
    }

    fn scaled(&self, scale: i32) -> Self::AffineView {
        AffineView::new(*self, scale, 0)
    }

    fn offset(&self, offset: i32) -> Self::AffineView {
        AffineView::new(*self, 1, offset)
    }
}

impl From<DomainId> for AffineView<DomainId> {
    fn from(value: DomainId) -> Self {
        AffineView::new(value, 1, 0)
    }
}

enum Rounding {
    Up,
    Down,
}

impl<Inner> AffineView<Inner> {
    pub fn new(inner: Inner, scale: i32, offset: i32) -> Self {
        AffineView {
            inner,
            scale,
            offset,
        }
    }

    /// Apply the inverse transformation of this view on a value, to go from the value in the domain
    /// of `self` to a value in the domain of `self.inner`.
    fn invert(&self, value: i32, rounding: Rounding) -> i32 {
        let inverted_translation = value - self.offset;

        // TODO: The source is taken from the standard library nightly implementation of these
        // methods. Once they are stabilized, these definitions can be removed.
        // Tracking issue: https://github.com/rust-lang/rust/issues/88581
        fn div_ceil(lhs: i32, rhs: i32) -> i32 {
            let d = lhs / rhs;
            let r = lhs % rhs;
            if (r > 0 && rhs > 0) || (r < 0 && rhs < 0) {
                d + 1
            } else {
                d
            }
        }

        fn div_floor(lhs: i32, rhs: i32) -> i32 {
            let d = lhs / rhs;
            let r = lhs % rhs;
            if (r > 0 && rhs < 0) || (r < 0 && rhs > 0) {
                d - 1
            } else {
                d
            }
        }

        match rounding {
            Rounding::Up => div_ceil(inverted_translation, self.scale),
            Rounding::Down => div_floor(inverted_translation, self.scale),
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
    type AffineView = Self;

    fn lower_bound(&self, assignment: &AssignmentsInteger) -> i32 {
        if self.scale < 0 {
            self.map(self.inner.upper_bound(assignment))
        } else {
            self.map(self.inner.lower_bound(assignment))
        }
    }

    fn upper_bound(&self, assignment: &AssignmentsInteger) -> i32 {
        if self.scale < 0 {
            self.map(self.inner.lower_bound(assignment))
        } else {
            self.map(self.inner.upper_bound(assignment))
        }
    }

    fn contains(&self, assignment: &AssignmentsInteger, value: i32) -> bool {
        if (value - self.offset) % self.scale == 0 {
            let inverted = self.invert(value, Rounding::Up);
            self.inner.contains(assignment, inverted)
        } else {
            false
        }
    }

    fn describe_domain(&self, assignment: &AssignmentsInteger) -> Vec<Predicate> {
        // The description should not actually change. It is a description of the domain as seen by
        // the solver, not as seen by the user of this view.
        self.inner.describe_domain(assignment)
    }

    fn remove(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain> {
        if (value - self.offset) % self.scale == 0 {
            let inverted = self.invert(value, Rounding::Up);
            self.inner.remove(assignment, inverted, reason)
        } else {
            Ok(())
        }
    }

    fn set_lower_bound(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain> {
        let inverted = self.invert(value, Rounding::Up);

        if self.scale >= 0 {
            self.inner.set_lower_bound(assignment, inverted, reason)
        } else {
            self.inner.set_upper_bound(assignment, inverted, reason)
        }
    }

    fn set_upper_bound(
        &self,
        assignment: &mut AssignmentsInteger,
        value: i32,
        reason: Option<ReasonRef>,
    ) -> Result<(), EmptyDomain> {
        let inverted = self.invert(value, Rounding::Down);

        if self.scale >= 0 {
            self.inner.set_upper_bound(assignment, inverted, reason)
        } else {
            self.inner.set_lower_bound(assignment, inverted, reason)
        }
    }

    fn watch_all(&self, watchers: &mut Watchers<'_>, mut events: EnumSet<IntDomainEvent>) {
        let bound = IntDomainEvent::LowerBound | IntDomainEvent::UpperBound;
        let intersection = events.intersection(bound);
        if intersection.len() == 1 && self.scale.is_negative() {
            events = events.symmetrical_difference(bound);
        }
        self.inner.watch_all(watchers, events);
    }

    fn unpack_event(&self, event: OpaqueDomainEvent) -> IntDomainEvent {
        if self.scale.is_negative() {
            match self.inner.unpack_event(event) {
                IntDomainEvent::LowerBound => IntDomainEvent::UpperBound,
                IntDomainEvent::UpperBound => IntDomainEvent::LowerBound,
                event => event,
            }
        } else {
            self.inner.unpack_event(event)
        }
    }

    fn scaled(&self, scale: i32) -> Self::AffineView {
        let mut result = self.clone();
        result.scale *= scale;
        result.offset *= scale;
        result
    }

    fn offset(&self, offset: i32) -> Self::AffineView {
        let mut result = self.clone();
        result.offset += offset;
        result
    }
}

impl<Var: std::fmt::Debug> std::fmt::Debug for AffineView<Var> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.scale == -1 {
            write!(f, "-")?;
        } else if self.scale != 1 {
            write!(f, "{} * ", self.scale)?;
        }

        write!(f, "({:?})", self.inner)?;

        match self.offset.cmp(&0) {
            Ordering::Less => write!(f, " - {}", -self.offset)?,
            Ordering::Equal => {}
            Ordering::Greater => write!(f, " + {}", self.offset)?,
        }

        Ok(())
    }
}

impl<Var: PredicateConstructor<Value = i32>> PredicateConstructor for AffineView<Var> {
    type Value = Var::Value;

    fn lower_bound_predicate(&self, bound: Self::Value) -> Predicate {
        let inverted_bound = self.invert(bound, Rounding::Up);

        if self.scale < 0 {
            self.inner.upper_bound_predicate(inverted_bound)
        } else {
            self.inner.lower_bound_predicate(inverted_bound)
        }
    }

    fn upper_bound_predicate(&self, bound: Self::Value) -> Predicate {
        let inverted_bound = self.invert(bound, Rounding::Down);

        if self.scale < 0 {
            self.inner.lower_bound_predicate(inverted_bound)
        } else {
            self.inner.upper_bound_predicate(inverted_bound)
        }
    }

    fn equality_predicate(&self, bound: Self::Value) -> Predicate {
        if (bound - self.offset) % self.scale == 0 {
            let inverted_bound = self.invert(bound, Rounding::Up);
            self.inner.equality_predicate(inverted_bound)
        } else {
            Predicate::False
        }
    }

    fn disequality_predicate(&self, bound: Self::Value) -> Predicate {
        if (bound - self.offset) % self.scale == 0 {
            let inverted_bound = self.invert(bound, Rounding::Up);
            self.inner.disequality_predicate(inverted_bound)
        } else {
            Predicate::True
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_an_affine_view() {
        let view = AffineView::new(DomainId::new(0), 3, 4);
        assert_eq!(3, view.scale);
        assert_eq!(4, view.offset);
        let scaled_view = view.scaled(6);
        assert_eq!(18, scaled_view.scale);
        assert_eq!(24, scaled_view.offset);
    }

    #[test]
    fn offsetting_an_affine_view() {
        let view = AffineView::new(DomainId::new(0), 3, 4);
        assert_eq!(3, view.scale);
        assert_eq!(4, view.offset);
        let scaled_view = view.offset(6);
        assert_eq!(3, scaled_view.scale);
        assert_eq!(10, scaled_view.offset);
    }

    #[test]
    fn affine_view_obtaining_a_bound_should_round_optimistically_in_inner_domain() {
        let domain = DomainId::new(0);
        let view = AffineView::new(domain, 2, 0);

        assert_eq!(
            domain.lower_bound_predicate(1),
            view.lower_bound_predicate(1)
        );

        assert_eq!(
            domain.lower_bound_predicate(-1),
            view.lower_bound_predicate(-3)
        );

        assert_eq!(
            domain.upper_bound_predicate(0),
            view.upper_bound_predicate(1)
        );

        assert_eq!(
            domain.upper_bound_predicate(-3),
            view.upper_bound_predicate(-5)
        );
    }
}
