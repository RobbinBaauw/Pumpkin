use std::fmt::Display;
use std::fmt::Formatter;

use itertools::Itertools;

use crate::engine::Assignments;
use crate::variables::AffineView;
use crate::variables::DomainId;
use crate::variables::IntegerVariable;
use crate::variables::TransformableVariable;

#[derive(Default, Debug, Clone)]
pub struct LinearLessOrEqual {
    pub lhs: Vec<(DomainId, i32)>,
    pub rhs: i32,
}

impl LinearLessOrEqual {
    pub fn contains_variable(&self, variable: DomainId) -> bool {
        self.lhs.iter().find(|(var, _)| *var == variable).is_some()
    }

    pub fn find_variable_scale(&self, variable: DomainId) -> Option<i32> {
        self.lhs
            .iter()
            .find(|(var, _)| *var == variable)
            .map(|(_, scale)| *scale)
    }

    pub fn to_vars(&self) -> Vec<AffineView<DomainId>> {
        self.lhs
            .iter()
            .map(|(var, scale)| var.scaled(*scale))
            .collect_vec()
    }

    fn lb_lhs_overflows(&self, assignments: &Assignments, trail_position: usize) -> bool {
        self.lhs.iter().any(|(var, scale)| {
            let bound = if *scale < 0 {
                var.upper_bound_at_trail_position(assignments, trail_position)
            } else {
                var.lower_bound_at_trail_position(assignments, trail_position)
            };

            scale.checked_mul(bound).is_none()
        })
    }

    pub fn lb_lhs(&self, assignments: &Assignments, trail_position: usize) -> i64 {
        self.lhs
            .iter()
            .map(|(var, scale)| {
                let scaled_var = var.scaled(*scale);
                scaled_var.lower_bound_at_trail_position(assignments, trail_position) as i64
            })
            .sum::<i64>()
    }

    pub fn slack(&self, assignments: &Assignments, trail_position: usize) -> i64 {
        (self.rhs as i64) - self.lb_lhs(assignments, trail_position)
    }

    pub fn is_conflicting(&self, assignments: &Assignments, trail_position: usize) -> bool {
        self.slack(assignments, trail_position) < 0
    }

    pub fn is_propagating(&self, assignments: &Assignments, trail_position: usize) -> bool {
        let lb_lhs = self.lb_lhs(assignments, trail_position);

        for (id, scale) in &self.lhs {
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

    pub fn overflows(&self, assignments: &Assignments, trail_index: usize) -> bool {
        if self.lb_lhs_overflows(assignments, trail_index) {
            return true;
        }

        let slack = self.slack(assignments, trail_index);
        for x_i in self.to_vars() {
            let bound: Result<i32, _> = (slack
                + x_i.lower_bound_at_trail_position(assignments, trail_index) as i64)
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

impl PartialEq for LinearLessOrEqual {
    fn eq(&self, other: &Self) -> bool {
        if self.rhs != other.rhs { return false; }

        let self_sorted = self.lhs.iter().sorted_by_key(|(var, _)| var.id);
        let other_sorted = other.lhs.iter().sorted_by_key(|(var, _)| var.id);
        self_sorted.eq(other_sorted)
    }
}
