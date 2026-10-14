//! Inbound MCP adapter over the operation registry.
//!
//! The registry is the one catalog of typed operations, playbooks, and
//! toolkit guides; every MCP surface searches, reads, and executes through
//! it. Each registered operation binds to its runtime owner: platform reads
//! and operator-account workflows to the `LocalPlatform` the host injects,
//! Creator, outreach, and mail workflows to `CreatorRuntime`. The adapter
//! owns no vault, browser session, platform account, provider client, or
//! business logic of its own.

mod accounts;
mod apps;
mod failure;
pub(crate) mod operation_registry;
mod qr_images;
mod server;
mod tools;

pub use accounts::{OperatorAccountWorkflow, PlatformRuntimeOperatorAccountWorkflow};
/// An external MCP Apps Host may embed the current generic inspector template
/// served by the MCP Apps overlay and drive its closed interaction kinds
/// through the App-only `onecrew_result_state` tool.
pub use apps::widget_template;
/// The closed vocabulary of presentation kinds the MCP App can render. A crate
/// that depends on this adapter and mints presentation kinds of its own checks
/// them against this list from its own tests, so the check runs on the
/// depending side of the edge rather than this crate inspecting its dependents.
pub use operation_registry::app_presentation_kinds;
#[doc(hidden)]
pub use operation_registry::result_contract_schema_manifest;
/// The registry compiled once per application graph, shared by the local MCP
/// service and the secret-free toolkit directory the Web reads.
pub use operation_registry::{OperationRegistryHandle, ToolkitDirectory, ToolkitOperationSummary};
pub use server::{
    LocalMcpService, McpAdapter, McpAdapterCompletionObserver, McpAdapterHandle, McpAdapterOptions,
    McpAdapterTaskCompletion, McpAdapterTaskFailureKind, McpAdapterUpdateControl,
    McpAdapterUpdatePause, McpHealthError, McpHealthErrorKind, McpHealthReport, McpSetupPrompt,
};
