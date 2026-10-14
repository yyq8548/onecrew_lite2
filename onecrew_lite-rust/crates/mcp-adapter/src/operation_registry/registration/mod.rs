//! Registration construction, policy validation, and sealed query facade.
mod builder;
mod error;
mod policy;
mod registry;
mod validation;

pub(crate) use builder::RegistryBuilder;
#[cfg(test)]
pub(super) use error::ParameterPointerRole;
pub(crate) use error::RegistryBuildError;
pub(crate) use policy::OperationPolicy;
pub(crate) use registry::OperationRegistry;
