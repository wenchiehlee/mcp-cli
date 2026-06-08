//! Rust library for connecting to MCP servers and invoking MCP tools.
//!
//! The crate also ships the `mcp-cli` binary. Library consumers can use
//! [`McpClient`] for the common workflow or the lower-level modules for direct
//! control over configuration, connections, and output formatting.

pub mod api;
pub mod client;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod daemon_client;
pub mod errors;
pub mod output;

pub use api::McpClient;
pub use client::{get_connection, HttpClient, McpConnection, StdioClient};
pub use config::{HttpServerConfig, McpServersConfig, ServerConfig, StdioServerConfig};
pub use errors::{CliError, ErrorCode};
pub use output::{ServerInfo, ToolInfo};

/// Convenient imports for common library usage.
pub mod prelude {
    pub use crate::{
        CliError, ErrorCode, HttpServerConfig, McpClient, McpConnection, McpServersConfig,
        ServerConfig, StdioServerConfig, ToolInfo,
    };
}
