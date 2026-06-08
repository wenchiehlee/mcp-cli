use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ClientError = 1,
    ServerError = 2,
    NetworkError = 3,
    AuthError = 4,
}

impl ErrorCode {
    pub fn exit_code(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone)]
pub struct CliError {
    pub code: ErrorCode,
    pub error_type: String,
    pub message: String,
    pub details: Option<String>,
    pub suggestion: Option<String>,
}

impl CliError {
    pub fn format(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Error [{}]: {}", self.error_type, self.message));
        if let Some(ref details) = self.details {
            lines.push(format!("  Details: {}", details));
        }
        if let Some(ref suggestion) = self.suggestion {
            lines.push(format!("  Suggestion: {}", suggestion));
        }
        lines.join("\n")
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

impl std::error::Error for CliError {}

// ============================================================================
// Config Errors
// ============================================================================

pub fn config_not_found_error(path: &str) -> CliError {
    CliError {
        code: ErrorCode::ClientError,
        error_type: "CONFIG_NOT_FOUND".to_string(),
        message: format!("Config file not found: {}", path),
        details: None,
        suggestion: Some(
            "Create mcp_servers.json with: { \"mcpServers\": { \"server-name\": { \"command\": \"...\" } } }".to_string(),
        ),
    }
}

pub fn config_search_error() -> CliError {
    CliError {
        code: ErrorCode::ClientError,
        error_type: "CONFIG_NOT_FOUND".to_string(),
        message: "No mcp_servers.json found in search paths".to_string(),
        details: Some(
            "Searched: ./mcp_servers.json, ~/.mcp_servers.json, ~/.config/mcp/mcp_servers.json".to_string(),
        ),
        suggestion: Some(
            "Create mcp_servers.json in current directory or use -c/--config to specify path".to_string(),
        ),
    }
}

pub fn config_invalid_json_error(path: &str, parse_error: &str) -> CliError {
    CliError {
        code: ErrorCode::ClientError,
        error_type: "CONFIG_INVALID_JSON".to_string(),
        message: format!("Invalid JSON in config file: {}", path),
        details: Some(parse_error.to_string()),
        suggestion: Some(
            "Check for syntax errors: missing commas, unquoted keys, trailing commas".to_string(),
        ),
    }
}

pub fn config_missing_field_error(path: &str) -> CliError {
    CliError {
        code: ErrorCode::ClientError,
        error_type: "CONFIG_MISSING_FIELD".to_string(),
        message: "Config file missing required \"mcpServers\" object".to_string(),
        details: Some(format!("File: {}", path)),
        suggestion: Some("Config must have structure: { \"mcpServers\": { ... } }".to_string()),
    }
}

// ============================================================================
// Server Errors
// ============================================================================

pub fn server_not_found_error(server_name: &str, available: &[String]) -> CliError {
    let available_list = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };

    let suggestion = if available.is_empty() {
        format!(
            "Add server to mcp_servers.json: {{ \"mcpServers\": {{ \"{}\": {{ ... }} }} }}",
            server_name
        )
    } else {
        format!(
            "Use one of: {}",
            available
                .iter()
                .map(|s| format!("mcp-cli {}", s))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    CliError {
        code: ErrorCode::ClientError,
        error_type: "SERVER_NOT_FOUND".to_string(),
        message: format!("Server \"{}\" not found in config", server_name),
        details: Some(format!("Available servers: {}", available_list)),
        suggestion: Some(suggestion),
    }
}

pub fn server_connection_error(server_name: &str, cause: &str) -> CliError {
    let mut suggestion =
        "Check server configuration and ensure the server process can start".to_string();

    if cause.contains("ENOENT") || cause.contains("not found") || cause.contains("No such file") {
        suggestion =
            "Command not found. Install the MCP server: npx -y @modelcontextprotocol/server-<name>".to_string();
    } else if cause.contains("ECONNREFUSED") || cause.contains("refused") {
        suggestion =
            "Server refused connection. Check if the server is running and URL is correct".to_string();
    } else if cause.contains("ETIMEDOUT") || cause.contains("timeout") {
        suggestion =
            "Connection timed out. Check network connectivity and server availability".to_string();
    } else if cause.contains("401") || cause.contains("Unauthorized") {
        suggestion = "Authentication required. Add Authorization header to config".to_string();
    } else if cause.contains("403") || cause.contains("Forbidden") {
        suggestion = "Access forbidden. Check credentials and permissions".to_string();
    }

    CliError {
        code: ErrorCode::NetworkError,
        error_type: "SERVER_CONNECTION_FAILED".to_string(),
        message: format!("Failed to connect to server \"{}\"", server_name),
        details: Some(cause.to_string()),
        suggestion: Some(suggestion),
    }
}

// ============================================================================
// Tool Errors
// ============================================================================

pub fn tool_not_found_error(
    tool_name: &str,
    server_name: &str,
    available_tools: Option<&[String]>,
) -> CliError {
    let details = available_tools.map(|tools| {
        let tool_list = tools
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more_count = if tools.len() > 5 {
            format!(" (+{} more)", tools.len() - 5)
        } else {
            "".to_string()
        };
        format!("Available tools: {}{}", tool_list, more_count)
    });

    CliError {
        code: ErrorCode::ClientError,
        error_type: "TOOL_NOT_FOUND".to_string(),
        message: format!("Tool \"{}\" not found in server \"{}\"", tool_name, server_name),
        details,
        suggestion: Some(format!("Run 'mcp-cli {}' to see all available tools", server_name)),
    }
}

pub fn tool_execution_error(tool_name: &str, server_name: &str, cause: &str) -> CliError {
    let mut suggestion = "Check tool arguments match the expected schema".to_string();

    if cause.contains("validation") || cause.contains("invalid_type") {
        suggestion = format!(
            "Run 'mcp-cli {}/{}' to see the input schema, then fix arguments",
            server_name, tool_name
        );
    } else if cause.contains("required") {
        suggestion = format!(
            "Missing required argument. Run 'mcp-cli {}/{}' to see required fields",
            server_name, tool_name
        );
    } else if cause.contains("permission") || cause.contains("denied") {
        suggestion = "Permission denied. Check file/resource permissions".to_string();
    } else if cause.contains("not found") || cause.contains("ENOENT") {
        suggestion = "Resource not found. Verify the path or identifier exists".to_string();
    }

    CliError {
        code: ErrorCode::ServerError,
        error_type: "TOOL_EXECUTION_FAILED".to_string(),
        message: format!("Tool \"{}\" execution failed", tool_name),
        details: Some(cause.to_string()),
        suggestion: Some(suggestion),
    }
}

pub fn tool_disabled_error(tool_name: &str, server_name: &str) -> CliError {
    CliError {
        code: ErrorCode::ClientError,
        error_type: "TOOL_DISABLED".to_string(),
        message: format!("Tool \"{}\" is disabled by configuration", tool_name),
        details: Some(format!(
            "Server \"{}\" has allowedTools/disabledTools filtering configured",
            server_name
        )),
        suggestion: Some(format!(
            "Check your mcp_servers.json config. Remove \"{}\" from disabledTools or add it to allowedTools.",
            tool_name
        )),
    }
}

// ============================================================================
// Argument Errors
// ============================================================================

pub fn invalid_target_error(target: &str) -> CliError {
    CliError {
        code: ErrorCode::ClientError,
        error_type: "INVALID_TARGET".to_string(),
        message: format!("Invalid target format: \"{}\"", target),
        details: Some("Expected format: server/tool".to_string()),
        suggestion: Some(
            "Use 'mcp-cli <server>/<tool> <json>' format, e.g., 'mcp-cli github/search_repos \'{\"query\":\"mcp\"}\''".to_string(),
        ),
    }
}

pub fn invalid_json_args_error(input: &str, parse_error: Option<&str>) -> CliError {
    let truncated = if input.len() > 100 {
        format!("{}...", &input[..100])
    } else {
        input.to_string()
    };

    let details = match parse_error {
        Some(pe) => format!("Parse error: {}", pe),
        None => format!("Input: {}", truncated),
    };

    CliError {
        code: ErrorCode::ClientError,
        error_type: "INVALID_JSON_ARGUMENTS".to_string(),
        message: "Invalid JSON in tool arguments".to_string(),
        details: Some(details),
        suggestion: Some(
            "Use valid JSON: '{\"path\": \"./file.txt\"}'. Run 'mcp-cli info <server> <tool>' for the schema.".to_string(),
        ),
    }
}

pub fn unknown_option_error(option: &str) -> CliError {
    let option_lower = option.to_lowercase();
    let option_clean = option_lower.trim_start_matches('-');

    let suggestion = match option_clean {
        "server" | "s" => "Server is a positional argument. Use 'mcp-cli info <server>'".to_string(),
        "tool" | "t" => "Tool is a positional argument. Use 'mcp-cli call <server> <tool>'".to_string(),
        "args" | "arguments" | "a" | "input" => "Pass JSON directly: 'mcp-cli call <server> <tool> '{\"key\": \"value\"}''".to_string(),
        "pattern" | "p" | "search" | "query" => "Use 'mcp-cli grep \"*pattern*\"'".to_string(),
        "call" | "run" | "exec" => "Use 'call' as a subcommand, not option: 'mcp-cli call <server> <tool>'".to_string(),
        "info" | "list" | "get" => "Use 'info' as a subcommand, not option: 'mcp-cli info <server>'".to_string(),
        _ => "Valid options: -c/--config, -j/--json, -d/--with-descriptions, -r/--raw".to_string(),
    };

    CliError {
        code: ErrorCode::ClientError,
        error_type: "UNKNOWN_OPTION".to_string(),
        message: format!("Unknown option: {}", option),
        details: None,
        suggestion: Some(suggestion),
    }
}

pub fn missing_argument_error(command: &str, argument: &str) -> CliError {
    let suggestion = match command {
        "call" => "Use 'mcp-cli call <server> <tool> '{\"key\": \"value\"}''",
        "grep" => "Use 'mcp-cli grep \"*pattern*\"'",
        "-c/--config" => "Use 'mcp-cli -c /path/to/mcp_servers.json'",
        _ => "Run 'mcp-cli --help' for usage examples",
    }
    .to_string();

    CliError {
        code: ErrorCode::ClientError,
        error_type: "MISSING_ARGUMENT".to_string(),
        message: format!("Missing required argument for {}: {}", command, argument),
        details: None,
        suggestion: Some(suggestion),
    }
}

// ============================================================================
// Subcommand Errors
// ============================================================================

pub fn ambiguous_command_error(server_name: &str, tool_name: &str, has_args: bool) -> CliError {
    let cmd = if has_args {
        format!("mcp-cli call {} {} '<json>'", server_name, tool_name)
    } else {
        format!("mcp-cli call {} {}", server_name, tool_name)
    };

    CliError {
        code: ErrorCode::ClientError,
        error_type: "AMBIGUOUS_COMMAND".to_string(),
        message: "Ambiguous command: did you mean to call a tool or view info?".to_string(),
        details: Some(format!(
            "Received: mcp-cli {} {}{}",
            server_name,
            tool_name,
            if has_args { " ..." } else { "" }
        )),
        suggestion: Some(format!(
            "Use '{}' to execute, or 'mcp-cli info {} {}' to view schema",
            cmd, server_name, tool_name
        )),
    }
}

pub fn unknown_subcommand_error(subcommand: &str) -> CliError {
    let suggested = match subcommand.to_lowercase().as_str() {
        "run" | "execute" | "exec" | "invoke" => Some("call"),
        "list" | "ls" | "get" | "show" | "describe" => Some("info"),
        "search" | "find" | "query" => Some("grep"),
        _ => None,
    };

    let valid_commands = "info, grep, call";

    let suggestion = match suggested {
        Some(s) => format!("Did you mean 'mcp-cli {}'?", s),
        None => "Use 'mcp-cli --help' to see available commands".to_string(),
    };

    CliError {
        code: ErrorCode::ClientError,
        error_type: "UNKNOWN_SUBCOMMAND".to_string(),
        message: format!("Unknown subcommand: \"{}\"", subcommand),
        details: Some(format!("Valid subcommands: {}", valid_commands)),
        suggestion: Some(suggestion),
    }
}

pub fn too_many_arguments_error(command: &str, received: usize, max: usize) -> CliError {
    CliError {
        code: ErrorCode::ClientError,
        error_type: "TOO_MANY_ARGUMENTS".to_string(),
        message: format!("Too many arguments for {}", command),
        details: Some(format!(
            "Received {} arguments, maximum is {}",
            received, max
        )),
        suggestion: Some("Run 'mcp-cli --help' for correct usage".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_formatting() {
        let err = config_not_found_error("/path/to/config");
        assert_eq!(err.code.exit_code(), 1);
        assert_eq!(err.error_type, "CONFIG_NOT_FOUND");
        let formatted = err.format();
        assert!(formatted.contains("Config file not found: /path/to/config"));
        assert!(formatted.contains("Suggestion:"));
    }

    #[test]
    fn test_server_not_found_error() {
        let err = server_not_found_error("my-server", &["srv1".to_string(), "srv2".to_string()]);
        assert_eq!(err.code.exit_code(), 1);
        assert_eq!(err.error_type, "SERVER_NOT_FOUND");
        assert!(err.format().contains("srv1, srv2"));
    }

    #[test]
    fn test_server_connection_error_enoent() {
        let err = server_connection_error("my-server", "ENOENT: command not found");
        assert_eq!(err.code.exit_code(), 3); // NetworkError is 3
        assert!(err.format().contains("Install the MCP server"));
    }

    #[test]
    fn test_unknown_option_error() {
        let err = unknown_option_error("--server");
        assert!(err.format().contains("Server is a positional argument"));
    }

    #[test]
    fn test_unknown_subcommand_error() {
        let err = unknown_subcommand_error("run");
        assert!(err.format().contains("Did you mean 'mcp-cli call'?"));
    }
}
