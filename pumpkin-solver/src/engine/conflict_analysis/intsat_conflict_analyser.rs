use std::collections::HashMap;
use itertools::Itertools;
use crate::basic_types::StoredConflictInfo;
use crate::engine::conflict_analysis::{AnalysisStep, ConflictAnalyser, ConflictAnalysisContext, ConflictAnalysisResult, ResolutionConflictAnalyser};
use crate::engine::constraint_satisfaction_solver::CoreExtractionResult;
use crate::engine::propagation::PropagatorId;
use crate::engine::propagation::store::PropagatorStore;

#[derive(Default, Debug)]
pub(crate) struct IntSatConflictAnalyser {
    resolution_analyser: ResolutionConflictAnalyser,
}

fn prop_to_linear_constraint(propagator_store: &PropagatorStore, id: PropagatorId) -> (Vec<(u32, i32)>, i32) {
    let prop = &propagator_store[id];

    if let Some((x, c)) = prop.get_linear_constraint() {
        let var_c = x.iter().map(|var| var.offset).sum::<i32>();

        let lhs = x.iter().map(|var| (var.id, var.scale)).collect_vec();
        let rhs = c - var_c;

        (lhs, rhs)
    } else {
        todo!("unsupported propagator")
    }
}

impl ConflictAnalyser for IntSatConflictAnalyser {
    fn conflict_analysis(&mut self, context: &mut ConflictAnalysisContext) -> ConflictAnalysisResult {
        // Perform cutting planes
        let conflicting_constraint = match context.solver_state.get_conflict_info() {
            StoredConflictInfo::Explanation { conjunction: _conjunction, propagator } => {
                prop_to_linear_constraint(context.propagator_store, *propagator)
            }
            _ => todo!("unsupported conflict"),
        };

        let start_index = context.assignments_integer.num_trail_entries();

        for trail_index in (0..start_index).rev() {
            let next_literal = context.assignments_integer.get_trail_entry(trail_index);
            let next_literal_id = next_literal.predicate.get_domain();
            let next_literal_reason = next_literal.reason.unwrap();

            let conflicting_var = conflicting_constraint.0.iter().find(|(id, _)| *id == next_literal_id.id);
            if conflicting_var.is_none() { continue; }

            let (_, conflicting_scale) = conflicting_var.unwrap();

            let propagator = context.reason_store.get_propagator(next_literal_reason);
            let prop_constraint = prop_to_linear_constraint(context.propagator_store, propagator);

            let (_, prop_scale) = prop_constraint.0.iter().find(|(id, _)| *id == next_literal_id.id).unwrap();

            let lcm_val = lcm(*conflicting_scale, *prop_scale);
            let mult_conf = lcm_val / conflicting_scale;
            let mult_prop = -lcm_val / prop_scale;

            let mut new_lhs: HashMap<_, _> = conflicting_constraint.0.iter().map(|(id, scale)| {
                (id, mult_conf * *scale)
            }).collect();

            prop_constraint.0.iter().for_each(|(id, scale)| {
                if !new_lhs.contains_key(id) { new_lhs.insert(id, 0); }

                let curr_scale = new_lhs.get_mut(id).unwrap();
                *curr_scale += mult_prop * scale;
            });

            let new_rhs = conflicting_constraint.1 * mult_conf + prop_constraint.1 * mult_prop;

            println!("{:?} <= {new_rhs}", new_lhs);
            println!("OEWEE");
        }

        // Perform resolution as backup
        self.resolution_analyser.conflict_analysis(context)
    }

    fn compute_clausal_core(&mut self, context: &mut ConflictAnalysisContext) -> CoreExtractionResult {
        self.resolution_analyser.compute_clausal_core(context)
    }

    fn get_conflict_reasons(&mut self, context: &mut ConflictAnalysisContext, on_analysis_step: &mut dyn FnMut(AnalysisStep)) {
        self.resolution_analyser.get_conflict_reasons(context, on_analysis_step)
    }
}

#[inline]
fn lcm(a: i32, b: i32) -> i32 {
    if a == 0 && b == 0 { return 0; }
    let gcd = gcd(a, b);
    let lcm = (a * (b / gcd)).abs();
    lcm
}

#[inline]
fn gcd(a: i32, b: i32) -> i32 {
    let mut m = a;
    let mut n = b;
    if m == 0 || n == 0 {
        return (m | n).abs();
    }

    // find common factors of 2
    let shift = (m | n).trailing_zeros();

    // The algorithm needs positive numbers, but the minimum value
    // can't be represented as a positive one.
    // It's also a power of two, so the gcd can be
    // calculated by bitshifting in that case

    // Assuming two's complement, the number created by the shift
    // is positive for all numbers except gcd = abs(min value)
    // The call to .abs() causes a panic in debug mode
    if m == i32::MIN || n == i32::MIN {
        return ((1 << shift) as i32).abs();
    }

    // guaranteed to be positive now, rest like unsigned algorithm
    m = m.abs();
    n = n.abs();

    // divide n and m by 2 until odd
    m >>= m.trailing_zeros();
    n >>= n.trailing_zeros();

    while m != n {
        if m > n {
            m -= n;
            m >>= m.trailing_zeros();
        } else {
            n -= m;
            n >>= n.trailing_zeros();
        }
    }
    m << shift
}