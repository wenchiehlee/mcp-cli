use crate::config::{McpServersConfig, ServerConfig};
use crate::errors::CliError;
use crate::output::ToolInfo;

/// High-level MCP client for library consumers.
///
/// `McpClient` owns an MCP server configuration and creates MCP connections on
/// demand. Each helper method closes the connection before returning.
#[derive(Debug, Clone)]
pub struct McpClient {
    config: McpServersConfig,
}

impl McpClient {
    /// Load configuration from an explicit path, `MCP_CONFIG_PATH`, or the
    /// default search paths used by the CLI.
    pub fn load(config_path: Option<&str>) -> Result<Self, CliError> {
        let config = crate::config::load_config(config_path)?;
        Ok(Self { config })
    }

    /// Build a client from an already parsed configuration.
    pub fn from_config(config: McpServersConfig) -> Self {
        Self { config }
    }

    /// Return the underlying configuration.
    pub fn config(&self) -> &McpServersConfig {
        &self.config
    }

    /// Return configured server names.
    pub fn server_names(&self) -> Vec<String> {
        crate::config::list_server_names(&self.config)
    }

    /// Return the configuration for one server.
    pub fn server_config(&self, server_name: &str) -> Result<ServerConfig, CliError> {
        crate::config::get_server_config(&self.config, server_name)
    }

    /// Open a connection to one configured server.
    pub async fn connect(
        &self,
        server_name: &str,
    ) -> Result<crate::client::McpConnection, CliError> {
        let server_config = self.server_config(server_name)?;
        crate::client::get_connection(server_name, &server_config).await
    }

    /// List tools exposed by one configured server.
    pub async fn list_tools(&self, server_name: &str) -> Result<Vec<ToolInfo>, CliError> {
        let connection = self.connect(server_name).await?;
        let result = connection.list_tools().await;
        let _ = connection.close().await;
        result
    }

    /// Return optional server instructions when the server supports them.
    pub async fn get_instructions(&self, server_name: &str) -> Result<Option<String>, CliError> {
        let connection = self.connect(server_name).await?;
        let result = connection.get_instructions().await;
        let _ = connection.close().await;
        result
    }

    /// Invoke a tool on one configured server.
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        let connection = self.connect(server_name).await?;
        let result = connection.call_tool(tool_name, args).await;
        let _ = connection.close().await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{McpServersConfig, ServerConfig, StdioServerConfig};
    use std::collections::HashMap;

    #[test]
    fn exposes_server_names_from_config() {
        let mut servers = HashMap::new();
        servers.insert(
            "filesystem".to_string(),
            ServerConfig::Stdio(StdioServerConfig {
                command: "npx".to_string(),
                args: Some(vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    ".".to_string(),
                ]),
                env: None,
                cwd: None,
                allowed_tools: None,
                disabled_tools: None,
            }),
        );

        let client = McpClient::from_config(McpServersConfig {
            mcp_servers: servers,
        });

        assert_eq!(client.server_names(), vec!["filesystem".to_string()]);
        assert!(matches!(
            client.server_config("filesystem"),
            Ok(ServerConfig::Stdio(_))
        ));
    }
}
