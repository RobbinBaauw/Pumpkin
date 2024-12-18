use std::fmt::Display;
use std::fmt::Formatter;
use std::slice::Iter;
use std::slice::IterMut;

use itertools::Itertools;

use crate::engine::Assignments;
use crate::variables::AffineView;
use crate::variables::DomainId;
use crate::variables::IntegerVariable;
use crate::variables::TransformableVariable;

#[derive(Default, Debug, Clone, Eq, Hash)]
pub struct LinearLessOrEqualLhs(pub Vec<(DomainId, i32)>);

impl LinearLessOrEqualLhs {
    pub(crate) fn contains_variable(&self, variable: DomainId) -> bool {
        self.iter().find(|(var, _)| *var == variable).is_some()
    }

    pub(crate) fn find_variable_scale(&self, variable: DomainId) -> Option<i32> {
        self.iter()
            .find(|(var, _)| *var == variable)
            .map(|(_, scale)| *scale)
    }

    pub(crate) fn to_vars(&self) -> Vec<AffineView<DomainId>> {
        self.iter()
            .map(|(var, scale)| var.scaled(*scale))
            .collect_vec()
    }

    fn lb_overflows(&self, assignments: &Assignments, trail_position: usize) -> bool {
        self.iter().any(|(var, scale)| {
            let bound = if *scale < 0 {
                var.upper_bound_at_trail_position(assignments, trail_position)
            } else {
                var.lower_bound_at_trail_position(assignments, trail_position)
            };

            scale.checked_mul(bound).is_none()
        })
    }

    pub(crate) fn lb(&self, assignments: &Assignments, trail_position: usize) -> i64 {
        self.iter()
            .map(|(var, scale)| {
                let scaled_var = var.scaled(*scale);
                scaled_var.lower_bound_at_trail_position(assignments, trail_position) as i64
            })
            .sum::<i64>()
    }

    pub(crate) fn lb_initial(&self, assignments: &Assignments) -> i64 {
        self.iter()
            .map(|(var, scale)| {
                let scaled_var = var.scaled(*scale);
                scaled_var.lower_bound_initial(assignments) as i64
            })
            .sum::<i64>()
    }

    pub(crate) fn ub(&self, assignments: &Assignments, trail_position: usize) -> i64 {
        self.iter()
            .map(|(var, scale)| {
                let scaled_var = var.scaled(*scale);
                scaled_var.upper_bound_at_trail_position(assignments, trail_position) as i64
            })
            .sum::<i64>()
    }

    pub(crate) fn ub_initial(&self, assignments: &Assignments) -> i64 {
        self.iter()
            .map(|(var, scale)| {
                let scaled_var = var.scaled(*scale);
                scaled_var.upper_bound_initial(assignments) as i64
            })
            .sum::<i64>()
    }

    pub fn iter(&self) -> Iter<'_, (DomainId, i32)> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, (DomainId, i32)> {
        self.0.iter_mut()
    }
}

impl From<Vec<(DomainId, i32)>> for LinearLessOrEqualLhs {
    fn from(value: Vec<(DomainId, i32)>) -> Self {
        LinearLessOrEqualLhs(value)
    }
}

impl PartialEq for LinearLessOrEqualLhs {
    fn eq(&self, other: &Self) -> bool {
        let self_sorted = self.iter().sorted_by_key(|(var, _)| var.id);
        let other_sorted = other.iter().sorted_by_key(|(var, _)| var.id);
        self_sorted.eq(other_sorted)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinearLessOrEqual {
    pub lhs: LinearLessOrEqualLhs,
    pub rhs: i32,
}

impl LinearLessOrEqual {
    pub(crate) fn new<L: Into<LinearLessOrEqualLhs>>(lhs: L, rhs: i32) -> Self {
        Self {
            lhs: lhs.into(),
            rhs,
        }
    }

    pub(crate) fn evaluate_at_trail_position(
        &self,
        assignments: &Assignments,
        trail_position: usize,
    ) -> Option<bool> {
        let ub_lhs = self.lhs.ub(assignments, trail_position);
        let lb_lhs = self.lhs.lb(assignments, trail_position);

        if ub_lhs <= self.rhs as i64 {
            Some(true)
        } else if lb_lhs > self.rhs as i64 {
            Some(false)
        } else {
            None
        }
    }

    pub(crate) fn slack(&self, assignments: &Assignments, trail_position: usize) -> i64 {
        (self.rhs as i64) - self.lhs.lb(assignments, trail_position)
    }

    pub(crate) fn is_conflicting(&self, assignments: &Assignments, trail_position: usize) -> bool {
        self.slack(assignments, trail_position) < 0
    }

    pub(crate) fn is_propagating(&self, assignments: &Assignments, trail_position: usize) -> bool {
        let lb_lhs = self.lhs.lb(assignments, trail_position);

        for (id, scale) in &self.lhs.0 {
            let x_i = id.scaled(*scale);

            let x_i_lower_bound =
                x_i.lower_bound_at_trail_position(assignments, trail_position) as i64;
            let x_i_upper_bound =
                x_i.upper_bound_at_trail_position(assignments, trail_position) as i64;

            let bound = (self.rhs as i64) - (lb_lhs - x_i_lower_bound);
            if x_i_upper_bound > bound {
                return true;
            }
        }

        false
    }

    pub(crate) fn overflows(&self, assignments: &Assignments, trail_position: usize) -> bool {
        if self.lhs.lb_overflows(assignments, trail_position) {
            return true;
        }

        let slack = self.slack(assignments, trail_position);
        for x_i in self.lhs.to_vars() {
            let bound: Result<i32, _> = (slack
                + x_i.lower_bound_at_trail_position(assignments, trail_position) as i64)
                .try_into();

            if bound.is_err() {
                return true;
            }
        }

        false
    }
}

impl Display for LinearLessOrEqual {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let lhs_mapped = self
            .lhs
            .0
            .iter()
            .sorted_by_key(|(var, _)| var.id)
            .filter_map(|(v, s)| {
                return if *s == 0 {
                    None
                } else if *s == 1 {
                    Some(format!("{v}"))
                } else if *s == -1 {
                    Some(format!("-{v}"))
                } else {
                    Some(format!("{s}{v}"))
                };
            })
            .join(" + ");
        let mut res = format!("{lhs_mapped} <= {:?}", self.rhs);
        if res.len() > 10000000 {
            res.truncate(300);
            write!(f, "{}...", res)
        } else {
            write!(f, "{}", res)
        }
    }
}
