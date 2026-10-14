//! Canonical account identity resolution, independent of provider discovery.
use crate::{IdentityResolution, ResolveCreatorIdentityInput};
impl crate::CreatorRuntime {
    pub fn resolve_creator_identity(
        &self,
        input: ResolveCreatorIdentityInput,
    ) -> Result<IdentityResolution, String> {
        self.repository.resolve_creator_identity(&input.candidate)
    }
}
