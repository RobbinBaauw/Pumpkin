use crate::engine::conflict_analysis::{ConflictAnalysisContext, ConflictResolveResult};

pub trait ConflictResolver {
    fn resolve_conflict(
        &mut self,
        context: &mut ConflictAnalysisContext,
    ) -> Option<ConflictResolveResult>;

    #[allow(clippy::result_unit_err)]
    fn process(
        &mut self,
        context: &mut ConflictAnalysisContext,
        learned_nogood: &Option<ConflictResolveResult>,
    ) -> Result<(), ()>;
}
