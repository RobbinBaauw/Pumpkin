use std::fmt::Display;
use std::fmt::Formatter;

use itertools::Itertools;

use crate::basic_types::linear_less_or_equal::LinearLessOrEqual;
use crate::basic_types::HashMap;
use crate::basic_types::PropositionalConjunction;
use crate::variables::DomainId;

#[derive(Clone, Debug)]
pub struct LearnedConstraintDomains(pub(crate) HashMap<DomainId, (i32, i32)>);

impl Display for LearnedConstraintDomains {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            self.0
                .iter()
                .map(|(id, domain)| format!("{}:({},{})", id.id, domain.0, domain.1))
                .join(" ")
                .as_str(),
        )
    }
}

#[derive(Debug, Clone)]
pub enum LearnedConstraintLogItem {
    NewConstraint {
        backjump_level: usize,
        learned_constraint: LinearLessOrEqual,
        learned_nogoods: PropositionalConjunction,
    },
    NewPropagator {
        propagator_id: u32,
        learned_constraint: LinearLessOrEqual,
    },
    ConstraintPropagation {
        propagator_id: u32,
        propagated_var: DomainId,
        domains_at_propagation: LearnedConstraintDomains,
    },
    ConstraintError {
        propagator_id: u32,
        domains_at_error: LearnedConstraintDomains,
    },
}

impl Display for LearnedConstraintLogItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Formats as CSV (event, propagator id, constraint, nogood, domains, backjump)
        match &self {
            LearnedConstraintLogItem::NewConstraint {
                learned_constraint,
                learned_nogoods,
                backjump_level,
            } => {
                write!(
                    f,
                    "NC|{learned_constraint}|{learned_nogoods}|{backjump_level}",
                )
            }
            LearnedConstraintLogItem::NewPropagator {
                propagator_id,
                learned_constraint,
            } => {
                write!(f, "NP|{propagator_id}|{learned_constraint}")
            }
            LearnedConstraintLogItem::ConstraintPropagation {
                propagator_id,
                domains_at_propagation,
                propagated_var,
            } => {
                write!(
                    f,
                    "CP|{propagator_id}|{domains_at_propagation}|{propagated_var}"
                )
            }
            LearnedConstraintLogItem::ConstraintError {
                propagator_id,
                domains_at_error,
            } => {
                write!(f, "CE|{propagator_id}|{domains_at_error}")
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LearnedConstraintLog {
    items: Vec<(u64, LearnedConstraintLogItem)>,
    num_conflicts: u64,
}

impl LearnedConstraintLog {
    pub(crate) fn update_num_conflicts(&mut self, num_conflicts: u64) {
        self.num_conflicts = num_conflicts;
    }

    pub(crate) fn log_item(&mut self, item: LearnedConstraintLogItem) {
        self.items.push((self.num_conflicts, item))
    }
}

impl Display for LearnedConstraintLog {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.items
            .iter()
            .try_for_each(|(conflict_count, item)| writeln!(f, "{conflict_count}|{item}"))
    }
}
