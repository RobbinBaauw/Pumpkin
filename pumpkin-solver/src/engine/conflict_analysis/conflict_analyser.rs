use std::fmt::Debug;
use crate::engine::conflict_analysis::{AnalysisStep, ConflictAnalysisContext};
use crate::engine::constraint_satisfaction_solver::CoreExtractionResult;
use crate::variables::Literal;

#[derive(Clone, Default, Debug)]
/// The outcome of clause learning.
pub(crate) struct ConflictAnalysisResult {
    /// The new learned clause with the propagating literal after backjumping at index 0 and the
    /// literal with the next highest decision level at index 1.
    pub learned_literals: Vec<Literal>,
    /// The decision level to backtrack to.
    pub backjump_level: usize,
}

pub(crate) trait ConflictAnalyser: Debug {
    fn conflict_analysis(&mut self, context: &mut ConflictAnalysisContext) -> ConflictAnalysisResult;

    fn compute_clausal_core(&mut self, context: &mut ConflictAnalysisContext, ) -> CoreExtractionResult;

    fn get_conflict_reasons(&mut self, context: &mut ConflictAnalysisContext, on_analysis_step: &mut dyn FnMut(AnalysisStep));
}
