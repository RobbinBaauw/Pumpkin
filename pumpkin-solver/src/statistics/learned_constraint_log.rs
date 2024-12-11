use std::fmt::Display;
use std::fmt::Formatter;

use crate::basic_types::linear_less_or_equal::LinearLessOrEqual;
use crate::basic_types::HashMap;
use crate::basic_types::PropositionalConjunction;
use crate::variables::DomainId;

#[derive(Debug, Clone)]
pub enum LearnedConstraintLogItem {
    NewConstraint {
        backjump_level: usize,
        learned_constraint: LinearLessOrEqual,
        learned_nogoods: PropositionalConjunction,
        domains_at_backjump: HashMap<DomainId, (i32, i32)>,
    },
    NewPropagator {
        propagator_id: u32,
        learned_constraint: LinearLessOrEqual,
    },
    ConstraintPropagation {
        propagator_id: u32,
    },
    ConstraintError {
        propagator_id: u32,
    },
}

impl Display for LearnedConstraintLogItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Formats as CSV (event, propagator id, constraint, nogood, domains, backjump)
        match &self {
            LearnedConstraintLogItem::NewConstraint {
                learned_constraint,
                learned_nogoods,
                domains_at_backjump,
                backjump_level,
            } => {
                write!(
                    f,
                    "NC,,{learned_constraint},{learned_nogoods},{:?},{backjump_level}",
                    domains_at_backjump
                )
            }
            LearnedConstraintLogItem::NewPropagator {
                propagator_id,
                learned_constraint,
            } => {
                write!(f, "NP,{propagator_id},{learned_constraint},,,")
            }
            LearnedConstraintLogItem::ConstraintPropagation { propagator_id } => {
                write!(f, "CP,{propagator_id},,,,")
            }
            LearnedConstraintLogItem::ConstraintError { propagator_id } => {
                write!(f, "CE,{propagator_id},,,,")
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
            .try_for_each(|(conflict_count, item)| writeln!(f, "{conflict_count},{item}"))
    }
}
