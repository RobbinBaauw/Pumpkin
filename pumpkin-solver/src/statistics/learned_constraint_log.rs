use std::fmt::Display;
use std::fmt::Formatter;

use itertools::Itertools;

use crate::basic_types::linear_less_or_equal::LinearLessOrEqual;
use crate::basic_types::HashMap;
use crate::basic_types::PropositionalConjunction;
use crate::variables::DomainId;

#[derive(Clone, Debug)]
pub struct VariableDomains(pub(crate) HashMap<DomainId, (i32, i32)>);

impl Display for VariableDomains {
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
    ConflictResult {
        learned_constraint: LinearLessOrEqual,
        learned_nogoods: PropositionalConjunction,
    },
    NewPropagator {
        propagator_id: u32,
        learned_constraint: LinearLessOrEqual,
    },
    NewNogood {
        nogood_id: u32,
        learned_constraint: LinearLessOrEqual,
    },
    ConstraintPropagation {
        propagator_id: u32,
        propagated_var: DomainId,
        domains_at_propagation: VariableDomains,
    },
    NogoodPropagation {
        nogood_id: u32,
        propagated_var: DomainId,
        domains_at_propagation: VariableDomains,
    },
    ConstraintError {
        propagator_id: u32,
        domains_at_error: VariableDomains,
    },
}

impl Display for LearnedConstraintLogItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Formats as CSV (event, propagator id, constraint, nogood, domains, backjump)
        match &self {
            LearnedConstraintLogItem::ConflictResult {
                learned_constraint,
                learned_nogoods,
            } => {
                write!(f, "CR|{learned_constraint}|{learned_nogoods}",)
            }
            LearnedConstraintLogItem::NewPropagator {
                propagator_id,
                learned_constraint,
            } => {
                write!(f, "NP|{propagator_id}|{learned_constraint}")
            }
            LearnedConstraintLogItem::NewNogood {
                nogood_id,
                learned_constraint,
            } => {
                write!(f, "NNG|{nogood_id}|{learned_constraint}")
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
            LearnedConstraintLogItem::NogoodPropagation {
                propagated_var,
                domains_at_propagation,
                nogood_id,
            } => {
                write!(
                    f,
                    "NGP|{nogood_id}|{domains_at_propagation}|{propagated_var}"
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
