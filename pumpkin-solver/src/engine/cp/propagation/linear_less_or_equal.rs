use std::fmt::{Display, Formatter};
use itertools::Itertools;
use crate::engine::Assignments;
use crate::variables::{AffineView, DomainId, IntegerVariable, TransformableVariable};

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
        self.lhs.iter().find(|(var, _)| *var == variable).map(|(_, scale)| *scale)
    }

    pub fn to_vars(&self) -> Vec<AffineView<DomainId>> {
        self.lhs.iter().map(|(var, scale)| var.scaled(*scale)).collect_vec()
    }

    fn lb_lhs(&self, assignments: &Assignments, trail_position: Option<usize>) -> i64 {
        self
            .lhs
            .iter()
            .map(|(var, scale)| {
                let scaled_var = var.scaled(*scale);
                if let Some(trail_position_act) = trail_position {
                    scaled_var.lower_bound_at_trail_position(assignments, trail_position_act) as i64
                } else {
                    scaled_var.lower_bound(assignments) as i64
                }
            })
            .sum::<i64>()
    }

    pub fn slack(&self, assignments: &Assignments, trail_position: Option<usize>) -> i64 {
        (self.rhs as i64) - self.lb_lhs(assignments, trail_position)
    }

    pub fn is_conflicting(&self, assignments: &Assignments, trail_position: Option<usize>) -> bool {
        self.slack(assignments, trail_position) < 0
    }

    pub fn is_propagating(&self, assignments: &Assignments, trail_position: Option<usize>) -> bool {
        let lb_lhs = self.lb_lhs(assignments, trail_position);

        for (id, scale) in &self.lhs {
            let x_i = id.scaled(*scale);

            let x_i_lower_bound = if let Some(trail_position_act) = trail_position {
                x_i.lower_bound_at_trail_position(assignments, trail_position_act) as i64
            } else {
                x_i.lower_bound(assignments) as i64
            };

            let x_i_upper_bound = if let Some(trail_position_act) = trail_position {
                x_i.upper_bound_at_trail_position(assignments, trail_position_act) as i64
            } else {
                x_i.upper_bound(assignments) as i64
            };

            let bound = (self.rhs as i64) - (lb_lhs - x_i_lower_bound);
            if x_i_upper_bound > bound {
                return true;
            }
        }

        false
    }
}

impl Display for LinearLessOrEqual {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let lhs_mapped = self.lhs.iter().sorted_by_key(|(var, _)| var.id).filter_map(|(v, s)| {
            return if *s == 0 {
                None
            } else if *s == 1 {
                Some(format!("{v}"))
            } else if *s == -1 {
                Some(format!("-{v}"))
            } else {
                Some(format!("{s}{v}"))
            }
        }).join(" + ");
        let mut res = format!("{lhs_mapped} <= {:?}", self.rhs);
        if res.len() > 10000000 {
            res.truncate(300);
            write!(f, "{}...", res)
        } else {
            write!(f, "{}", res)
        }
    }
}