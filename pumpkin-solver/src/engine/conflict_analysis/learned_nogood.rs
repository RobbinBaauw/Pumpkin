use crate::predicates::Predicate;
use crate::propagators::linear_less_or_equal::LinearLessOrEqualPropagator;
use crate::variables::{AffineView, DomainId};

#[derive(Clone, Debug)]
pub struct LearnedConstraint {
    pub(crate) learned_constraint: Box<LinearLessOrEqualPropagator<AffineView<DomainId>>>,
    pub(crate) backjump_level: usize,
}

#[derive(Clone, Debug)]
pub struct LearnedNogood {
    pub(crate) predicates: Vec<Predicate>,
    pub(crate) backjump_level: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum ConflictResolveResult {
    Nogood(LearnedNogood),
    Constraint(LearnedConstraint)
}
