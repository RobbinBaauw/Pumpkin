use crate::basic_types::KeyedVec;
use crate::engine::conflict_analysis::{AnalysisStep, ConflictAnalyser, ConflictAnalysisContext, ConflictAnalysisResult, RecursiveMinimiser, SemanticMinimiser};
use crate::engine::constraint_satisfaction_solver::CoreExtractionResult;
use crate::variables::PropositionalVariable;

#[derive(Default, Debug)]
pub(crate) struct IntSatConflictAnalyser {
    // data structures used for conflict analysis
    seen: KeyedVec<PropositionalVariable, bool>,
    analysis_result: ConflictAnalysisResult,

    /// A clause minimiser which uses a recursive minimisation approach to remove dominated
    /// literals (see [`RecursiveMinimiser`]).
    recursive_minimiser: RecursiveMinimiser,
    /// A clause minimiser which uses a semantic minimisation approach (see [`SemanticMinimiser`]).
    semantic_minimiser: SemanticMinimiser,
}

impl ConflictAnalyser for IntSatConflictAnalyser {
    fn conflict_analysis(&mut self, context: &mut ConflictAnalysisContext) -> ConflictAnalysisResult {
        todo!()
    }

    fn compute_clausal_core(&mut self, context: &mut ConflictAnalysisContext) -> CoreExtractionResult {
        todo!()
    }

    fn get_conflict_reasons(&mut self, context: &mut ConflictAnalysisContext, on_analysis_step: &mut dyn FnMut(AnalysisStep)) {
        todo!()
    }
}