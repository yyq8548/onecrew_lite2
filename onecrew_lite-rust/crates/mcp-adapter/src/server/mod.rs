mod health;
mod lifecycle;
mod local;
mod setup;
mod surface;
mod update_pause;

pub use health::{McpHealthError, McpHealthErrorKind, McpHealthReport};
pub use lifecycle::{
    McpAdapter, McpAdapterCompletionObserver, McpAdapterHandle, McpAdapterOptions,
    McpAdapterTaskCompletion, McpAdapterTaskFailureKind,
};
pub use local::LocalMcpService;
pub use setup::McpSetupPrompt;
#[cfg(test)]
pub(crate) use surface::surface_instructions;
pub(crate) use surface::{McpAdapterServer, McpSurface};
pub use update_pause::{McpAdapterUpdateControl, McpAdapterUpdatePause};

#[cfg(test)]
mod v2_adapter_tests;
