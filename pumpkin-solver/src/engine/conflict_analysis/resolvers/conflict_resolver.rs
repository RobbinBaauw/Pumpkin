use crate::engine::conflict_analysis::{ConflictAnalysisNogoodContext, ConflictResolveResult};

pub trait ConflictResolver {
    fn resolve_conflict(
        &mut self,
        context: &mut ConflictAnalysisNogoodContext,
    ) -> Option<ConflictResolveResult>;

    #[allow(clippy::result_unit_err)]
    fn process(
        &mut self,
        context: &mut ConflictAnalysisNogoodContext,
        learned_nogood: &Option<ConflictResolveResult>,
    ) -> Result<(), ()>;
}
