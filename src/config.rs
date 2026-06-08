use crate::errors::{
    config_invalid_json_error, config_missing_field_error, config_not_found_error,
    config_search_error, ErrorCode,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// Types
// ============================================================================

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ServerConfig {
    Stdio(StdioServerConfig),
    Http(HttpServerConfig),
}

impl ServerConfig {
    pub fn allowed_tools(&self) -> &Option<Vec<String>> {
        match self {
            ServerConfig::Stdio(c) => &c.allowed_tools,
            ServerConfig::Http(c) => &c.allowed_tools,
        }
    }

    pub fn disabled_tools(&self) -> &Option<Vec<String>> {
        match self {
            ServerConfig::Stdio(c) => &c.disabled_tools,
            ServerConfig::Http(c) => &c.disabled_tools,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StdioServerConfig {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub cwd: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub disabled_tools: Option<Vec<String>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HttpServerConfig {
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub timeout: Option<u64>,
    pub allowed_tools: Option<Vec<String>>,
    pub disabled_tools: Option<Vec<String>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpServersConfig {
    pub mcp_servers: HashMap<String, ServerConfig>,
}

// ============================================================================
// Tool Filtering
// ============================================================================

/// Convert a glob pattern containing * and ? into a regex.
fn matches_pattern(name: &str, pattern: &str) -> bool {
    // Convert glob to regex
    let mut regex_pattern = String::new();
    for c in pattern.chars() {
        match c {
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                regex_pattern.push('\\');
                regex_pattern.push(c);
            }
            '*' => {
                regex_pattern.push_str(".*");
            }
            '?' => {
                regex_pattern.push('.');
            }
            _ => {
                regex_pattern.push(c);
            }
        }
    }

    if let Ok(re) = regex::RegexBuilder::new(&format!("^{}$", regex_pattern))
        .case_insensitive(true)
        .build()
    {
        re.is_match(name)
    } else {
        false
    }
}

fn matches_any_pattern(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pat| matches_pattern(name, pat))
}

pub fn is_tool_allowed(tool_name: &str, config: &ServerConfig) -> bool {
    let disabled_tools = config.disabled_tools();
    if let Some(disabled) = disabled_tools {
        if !disabled.is_empty() && matches_any_pattern(tool_name, disabled) {
            return false;
        }
    }

    let allowed_tools = config.allowed_tools();
    if let Some(allowed) = allowed_tools {
        if !allowed.is_empty() {
            return matches_any_pattern(tool_name, allowed);
        }
    }

    true
}

pub fn filter_tools<T: AsRef<str>>(tools: &[T], config: &ServerConfig) -> Vec<T>
where
    T: Clone,
{
    tools
        .iter()
        .filter(|t| is_tool_allowed(t.as_ref(), config))
        .cloned()
        .collect()
}

// ============================================================================
// Environment & Constants
// ============================================================================

pub const DEFAULT_TIMEOUT_SECONDS: u64 = 1800; // 30 minutes
pub const DEFAULT_TIMEOUT_MS: u64 = DEFAULT_TIMEOUT_SECONDS * 1000;
pub const DEFAULT_CONCURRENCY: usize = 5;
pub const DEFAULT_MAX_RETRIES: u32 = 3;
pub const DEFAULT_RETRY_DELAY_MS: u64 = 1000;
pub const DEFAULT_DAEMON_TIMEOUT_SECONDS: u64 = 60;

pub fn debug(message: &str) {
    if env::var("MCP_DEBUG").is_ok() {
        eprintln!("[mcp-cli] {}", message);
    }
}

pub fn get_timeout_ms() -> u64 {
    if let Ok(val) = env::var("MCP_TIMEOUT") {
        if let Ok(seconds) = val.parse::<u64>() {
            if seconds > 0 {
                return seconds * 1000;
            }
        }
    }
    DEFAULT_TIMEOUT_MS
}

pub fn get_concurrency_limit() -> usize {
    if let Ok(val) = env::var("MCP_CONCURRENCY") {
        if let Ok(limit) = val.parse::<usize>() {
            if limit > 0 {
                return limit;
            }
        }
    }
    DEFAULT_CONCURRENCY
}

pub fn get_max_retries() -> u32 {
    if let Ok(val) = env::var("MCP_MAX_RETRIES") {
        if let Ok(retries) = val.parse::<u32>() {
            return retries;
        }
    }
    DEFAULT_MAX_RETRIES
}

pub fn get_retry_delay_ms() -> u64 {
    if let Ok(val) = env::var("MCP_RETRY_DELAY") {
        if let Ok(delay) = val.parse::<u64>() {
            if delay > 0 {
                return delay;
            }
        }
    }
    DEFAULT_RETRY_DELAY_MS
}

pub fn is_daemon_enabled() -> bool {
    env::var("MCP_NO_DAEMON").unwrap_or_default() != "1"
}

pub fn get_daemon_timeout_ms() -> u64 {
    if let Ok(val) = env::var("MCP_DAEMON_TIMEOUT") {
        if let Ok(seconds) = val.parse::<u64>() {
            if seconds > 0 {
                return seconds * 1000;
            }
        }
    }
    DEFAULT_DAEMON_TIMEOUT_SECONDS * 1000
}

pub fn get_socket_dir() -> PathBuf {
    #[cfg(unix)]
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = "unknown";

    PathBuf::from(format!("/tmp/mcp-cli-{}", uid))
}

pub fn get_socket_path(server_name: &str) -> PathBuf {
    get_socket_dir().join(format!("{}.sock", server_name))
}

pub fn get_pid_path(server_name: &str) -> PathBuf {
    get_socket_dir().join(format!("{}.pid", server_name))
}

// ============================================================================
// Hashing Config
// ============================================================================

pub fn get_config_hash(config: &ServerConfig) -> String {
    // Top-level key sorting for config serialization
    // This replicates TS's JSON.stringify(config, Object.keys(config).sort())
    let value = serde_json::to_value(config).unwrap_or(serde_json::Value::Null);

    let mut map = BTreeMap::new();
    if let serde_json::Value::Object(m) = value {
        for (k, v) in m {
            map.insert(k, v);
        }
    }

    let serialized = serde_json::to_string(&map).unwrap_or_default();

    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let hash = hasher.finalize();
    hex::encode(hash)[..16].to_string()
}

// ============================================================================
// Env substitution
// ============================================================================

fn is_strict_env_mode() -> bool {
    let value = env::var("MCP_STRICT_ENV").unwrap_or_default().to_lowercase();
    value != "false" && value != "0"
}

fn substitute_env_vars(value: &str) -> Result<String, crate::errors::CliError> {
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut missing_vars = Vec::new();

    let result = re.replace_all(value, |caps: &regex::Captures| {
        let var_name = &caps[1];
        match env::var(var_name) {
            Ok(v) => v,
            Err(_) => {
                missing_vars.push(var_name.to_string());
                String::new()
            }
        }
    });

    if !missing_vars.is_empty() {
        let var_list = missing_vars
            .iter()
            .map(|v| format!("${{{}}}", v))
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!(
            "Missing environment variable{}: {}",
            if missing_vars.len() > 1 { "s" } else { "" },
            var_list
        );

        if is_strict_env_mode() {
            return Err(crate::errors::CliError {
                code: ErrorCode::ClientError,
                error_type: "MISSING_ENV_VAR".to_string(),
                message,
                details: Some("Referenced in config but not set in environment".to_string()),
                suggestion: Some(format!(
                    "Set the variable(s) before running: export {}=\"value\" or set MCP_STRICT_ENV=false to use empty values",
                    missing_vars[0]
                )),
            });
        } else {
            eprintln!("[mcp-cli] Warning: {}", message);
        }
    }

    Ok(result.into_owned())
}

fn substitute_value(
    val: serde_json::Value,
) -> Result<serde_json::Value, crate::errors::CliError> {
    match val {
        serde_json::Value::String(s) => {
            let replaced = substitute_env_vars(&s)?;
            Ok(serde_json::Value::String(replaced))
        }
        serde_json::Value::Array(arr) => {
            let mut new_arr = Vec::new();
            for item in arr {
                new_arr.push(substitute_value(item)?);
            }
            Ok(serde_json::Value::Array(new_arr))
        }
        serde_json::Value::Object(obj) => {
            let mut new_obj = serde_json::Map::new();
            for (k, v) in obj {
                new_obj.insert(k, substitute_value(v)?);
            }
            Ok(serde_json::Value::Object(new_obj))
        }
        _ => Ok(val),
    }
}

// ============================================================================
// Config Loader
// ============================================================================

fn get_default_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. Current directory
    paths.push(PathBuf::from("./mcp_servers.json"));

    // 2. Home directory variants
    if let Ok(home) = env::var("HOME") {
        let home_path = Path::new(&home);
        paths.push(home_path.join(".mcp_servers.json"));
        paths.push(home_path.join(".config/mcp/mcp_servers.json"));
    }

    paths
}

pub fn load_config(explicit_path: Option<&str>) -> Result<McpServersConfig, crate::errors::CliError> {
    let mut config_path: Option<PathBuf> = None;

    if let Some(path) = explicit_path {
        config_path = Some(PathBuf::from(path));
    } else if let Ok(env_path) = env::var("MCP_CONFIG_PATH") {
        config_path = Some(PathBuf::from(env_path));
    }

    if let Some(path) = config_path {
        if !path.exists() {
            return Err(config_not_found_error(&path.to_string_lossy()));
        }
        config_path = Some(path);
    } else {
        // Search default paths
        for path in get_default_config_paths() {
            if path.exists() {
                config_path = Some(path);
                break;
            }
        }

        if config_path.is_none() {
            return Err(config_search_error());
        }
    }

    let actual_path = config_path.unwrap();
    let content = fs::read_to_string(&actual_path)
        .map_err(|e| config_invalid_json_error(&actual_path.to_string_lossy(), &e.to_string()))?;

    // Parse into standard JSON Value first to validate and substitute environment variables
    let mut json_val: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        config_invalid_json_error(&actual_path.to_string_lossy(), &e.to_string())
    })?;

    // Validate top level schema has mcpServers object
    let mcp_servers_field = json_val.get("mcpServers");
    if mcp_servers_field.is_none() || !mcp_servers_field.unwrap().is_object() {
        return Err(config_missing_field_error(&actual_path.to_string_lossy()));
    }

    // Substitute environment variables recursively
    json_val = substitute_value(json_val)?;

    // Deserialize into the struct
    let config: McpServersConfig = serde_json::from_value(json_val).map_err(|e| {
        config_invalid_json_error(&actual_path.to_string_lossy(), &e.to_string())
    })?;

    if config.mcp_servers.is_empty() {
        eprintln!("[mcp-cli] Warning: No servers configured in mcpServers. Add server configurations to use MCP tools.");
    }

    // Validate individual servers
    for (name, server_config) in &config.mcp_servers {
        match server_config {
            ServerConfig::Stdio(sc) => {
                if sc.command.is_empty() {
                    return Err(crate::errors::CliError {
                        code: ErrorCode::ClientError,
                        error_type: "CONFIG_INVALID_SERVER".to_string(),
                        message: format!("Server \"{}\" has empty command", name),
                        details: Some("command must be a non-empty string".to_string()),
                        suggestion: Some("Provide a valid command, e.g. \"npx\"".to_string()),
                    });
                }
            }
            ServerConfig::Http(hc) => {
                if hc.url.is_empty() {
                    return Err(crate::errors::CliError {
                        code: ErrorCode::ClientError,
                        error_type: "CONFIG_INVALID_SERVER".to_string(),
                        message: format!("Server \"{}\" has empty URL", name),
                        details: Some("url must be a non-empty string".to_string()),
                        suggestion: Some("Provide a valid URL, e.g. \"https://example.com/mcp\"".to_string()),
                    });
                }
            }
        }
    }

    Ok(config)
}

pub fn get_server_config(
    config: &McpServersConfig,
    server_name: &str,
) -> Result<ServerConfig, crate::errors::CliError> {
    match config.mcp_servers.get(server_name) {
        Some(server) => Ok(server.clone()),
        None => {
            let available: Vec<String> = config.mcp_servers.keys().cloned().collect();
            Err(crate::errors::server_not_found_error(server_name, &available))
        }
    }
}

pub fn list_server_names(config: &McpServersConfig) -> Vec<String> {
    config.mcp_servers.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("read_file", "read_file"));
        assert!(matches_pattern("read_file", "read_*"));
        assert!(matches_pattern("read_file", "*_file"));
        assert!(matches_pattern("read_file", "*"));
        assert!(matches_pattern("read_file", "r?ad_file"));
        assert!(!matches_pattern("read_file", "write_*"));
        // Case insensitivity
        assert!(matches_pattern("Read_File", "read_*"));
    }

    #[test]
    fn test_is_tool_allowed() {
        // 1. No allowed or disabled specified -> allowed
        let config_default = ServerConfig::Stdio(StdioServerConfig {
            command: "node".to_string(),
            args: None,
            env: None,
            cwd: None,
            allowed_tools: None,
            disabled_tools: None,
        });
        assert!(is_tool_allowed("read_file", &config_default));

        // 2. Only allowed_tools specified
        let config_allowed = ServerConfig::Stdio(StdioServerConfig {
            command: "node".to_string(),
            args: None,
            env: None,
            cwd: None,
            allowed_tools: Some(vec!["read_*".to_string(), "list_*".to_string()]),
            disabled_tools: None,
        });
        assert!(is_tool_allowed("read_file", &config_allowed));
        assert!(is_tool_allowed("list_directory", &config_allowed));
        assert!(!is_tool_allowed("write_file", &config_allowed));

        // 3. Only disabled_tools specified
        let config_disabled = ServerConfig::Stdio(StdioServerConfig {
            command: "node".to_string(),
            args: None,
            env: None,
            cwd: None,
            allowed_tools: None,
            disabled_tools: Some(vec!["write_*".to_string()]),
        });
        assert!(is_tool_allowed("read_file", &config_disabled));
        assert!(!is_tool_allowed("write_file", &config_disabled));

        // 4. Both specified (disabled has higher priority)
        let config_both = ServerConfig::Stdio(StdioServerConfig {
            command: "node".to_string(),
            args: None,
            env: None,
            cwd: None,
            allowed_tools: Some(vec!["read_*".to_string(), "write_*".to_string()]),
            disabled_tools: Some(vec!["write_secure".to_string()]),
        });
        assert!(is_tool_allowed("read_file", &config_both));
        assert!(is_tool_allowed("write_file", &config_both));
        assert!(!is_tool_allowed("write_secure", &config_both));
    }

    #[test]
    fn test_substitute_env_vars() {
        env::set_var("TEST_VAR_XYZ", "hello_world");
        let input = "path/${TEST_VAR_XYZ}/file.txt";
        let output = substitute_env_vars(input).unwrap();
        assert_eq!(output, "path/hello_world/file.txt");

        // Non-strict missing env var returns empty string without error
        env::set_var("MCP_STRICT_ENV", "false");
        let missing = "path/${NONEXISTENT_VAR_ABC}/file.txt";
        let output_missing = substitute_env_vars(missing).unwrap();
        assert_eq!(output_missing, "path//file.txt");

        // Strict missing env var returns error
        env::set_var("MCP_STRICT_ENV", "true");
        let result_err = substitute_env_vars(missing);
        assert!(result_err.is_err());
    }

    #[test]
    fn test_get_config_hash() {
        let config_1 = ServerConfig::Stdio(StdioServerConfig {
            command: "node".to_string(),
            args: Some(vec!["arg1".to_string(), "arg2".to_string()]),
            env: None,
            cwd: None,
            allowed_tools: None,
            disabled_tools: None,
        });

        let config_2 = ServerConfig::Stdio(StdioServerConfig {
            command: "node".to_string(),
            args: Some(vec!["arg1".to_string(), "arg2".to_string()]),
            env: None,
            cwd: None,
            allowed_tools: None,
            disabled_tools: None,
        });

        // Config hash should be deterministic and identical for same content
        let hash_1 = get_config_hash(&config_1);
        let hash_2 = get_config_hash(&config_2);
        assert_eq!(hash_1, hash_2);
        assert_eq!(hash_1.len(), 16);
    }

    #[test]
    fn test_load_config_valid() {
        use std::fs::File;
        use std::io::Write;

        let mut rng = rand::thread_rng();
        let rand_val: u32 = rand::Rng::gen(&mut rng);
        let file_path = std::env::temp_dir().join(format!("mcp_servers_test_{}.json", rand_val));
        let mut file = File::create(&file_path).unwrap();

        let content = r#"{
            "mcpServers": {
                "test-server": {
                    "command": "echo",
                    "args": ["hello"]
                }
            }
        }"#;
        file.write_all(content.as_bytes()).unwrap();

        let config = load_config(Some(file_path.to_str().unwrap())).unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        let srv = config.mcp_servers.get("test-server").unwrap();
        match srv {
            ServerConfig::Stdio(sc) => {
                assert_eq!(sc.command, "echo");
                assert_eq!(sc.args.as_ref().unwrap()[0], "hello");
            }
            _ => panic!("Expected Stdio configuration"),
        }

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn test_load_config_invalid() {
        use std::fs::File;
        use std::io::Write;

        let mut rng = rand::thread_rng();
        let rand_val: u32 = rand::Rng::gen(&mut rng);
        let file_path = std::env::temp_dir().join(format!("mcp_servers_invalid_{}.json", rand_val));
        let mut file = File::create(&file_path).unwrap();

        let content = r#"{
            "mcpServers": {
                "test-server": {
                    "command": ""
                }
            }
        }"#;
        file.write_all(content.as_bytes()).unwrap();

        let result = load_config(Some(file_path.to_str().unwrap()));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_type, "CONFIG_INVALID_SERVER");

        let _ = std::fs::remove_file(file_path);
    }
}

