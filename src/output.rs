use crate::config::ServerConfig;
use std::io::IsTerminal;

// ANSI color codes
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

pub struct ServerInfo {
    pub name: String,
    pub tools: Vec<ToolInfo>,
    pub instructions: Option<String>,
}

fn should_colorize() -> bool {
    std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err()
}

fn color(text: &str, color_code: &str) -> String {
    if !should_colorize() {
        return text.to_string();
    }
    format!("{}{}{}", color_code, text, RESET)
}

pub fn format_server_list(servers: &[ServerInfo], with_descriptions: bool) -> String {
    let mut lines = Vec::new();

    for server in servers {
        lines.push(color(&server.name, &format!("{}{}", BOLD, CYAN)));

        if let Some(ref inst) = server.instructions {
            let inst_lines: Vec<&str> = inst.split('\n').collect();
            let first_line = if inst_lines[0].len() > 100 {
                &inst_lines[0][..100]
            } else {
                inst_lines[0]
            };
            let suffix = if inst_lines.len() > 1 || inst_lines[0].len() > 100 {
                "..."
            } else {
                ""
            };
            lines.push(format!(
                "  {}",
                color(&format!("Instructions: {}{}", first_line, suffix), DIM)
            ));
        }

        for tool in &server.tools {
            if with_descriptions {
                if let Some(ref desc) = tool.description {
                    lines.push(format!("  • {} - {}", tool.name, color(desc, DIM)));
                } else {
                    lines.push(format!("  • {}", tool.name));
                }
            } else {
                lines.push(format!("  • {}", tool.name));
            }
        }

        lines.push(String::new()); // Empty line between servers
    }

    let joined = lines.join("\n");
    joined.trim_end().to_string()
}

pub struct SearchResult {
    pub server: String,
    pub tool: ToolInfo,
}

pub fn format_search_results(results: &[SearchResult], _with_descriptions: bool) -> String {
    let mut lines = Vec::new();

    for result in results {
        let server_colored = color(&result.server, CYAN);
        let tool_colored = color(&result.tool.name, GREEN);

        if let Some(ref desc) = result.tool.description {
            lines.push(format!(
                "{} {} {}",
                server_colored,
                tool_colored,
                color(desc, DIM)
            ));
        } else {
            lines.push(format!("{} {}", server_colored, tool_colored));
        }
    }

    lines.join("\n")
}

pub fn format_server_details(
    server_name: &str,
    config: &ServerConfig,
    tools: &[ToolInfo],
    with_descriptions: bool,
    instructions: Option<&str>,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("{}: {}", color("Server", BOLD), color(server_name, CYAN)));

    match config {
        ServerConfig::Http(hc) => {
            lines.push(format!("{}: HTTP", color("Transport", BOLD)));
            lines.push(format!("{}: {}", color("URL", BOLD), hc.url));
        }
        ServerConfig::Stdio(sc) => {
            lines.push(format!("{}: stdio", color("Transport", BOLD)));
            let args_str = sc.args.as_ref().map(|a| a.join(" ")).unwrap_or_default();
            lines.push(format!(
                "{}: {} {}",
                color("Command", BOLD),
                sc.command,
                args_str
            ));
        }
    }

    if let Some(inst) = instructions {
        lines.push(String::new());
        lines.push(color("Instructions:", BOLD));
        let indented = inst
            .split('\n')
            .map(|l| format!("  {}", l))
            .collect::<Vec<_>>()
            .join("\n");
        lines.push(indented);
    }

    lines.push(String::new());
    lines.push(format!("{}:", color(&format!("Tools ({})", tools.len()), BOLD)));

    for tool in tools {
        lines.push(format!("  {}", color(&tool.name, GREEN)));
        if with_descriptions {
            if let Some(ref desc) = tool.description {
                lines.push(format!("    {}", color(desc, DIM)));
            }
        }

        // Show parameters from schema
        if let Some(obj) = tool.input_schema.as_object() {
            if let Some(properties) = obj.get("properties").and_then(|p| p.as_object()) {
                lines.push(format!("    {}", color("Parameters:", YELLOW)));
                let required_list: Vec<&str> = obj
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|val| val.as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                for (name, prop) in properties {
                    let required = if required_list.contains(&name.as_str()) {
                        "required"
                    } else {
                        "optional"
                    };
                    let ty = prop
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("any");
                    let desc = if with_descriptions {
                        prop.get("description")
                            .and_then(|d| d.as_str())
                            .map(|d| format!(" - {}", d))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    lines.push(format!(
                        "      • {} ({}, {}){}",
                        name, ty, required, desc
                    ));
                }
            }
        }
        lines.push(String::new());
    }

    lines.join("\n").trim_end().to_string()
}

pub fn format_tool_schema(server_name: &str, tool: &ToolInfo) -> String {
    let mut lines = Vec::new();

    lines.push(format!("{}: {}", color("Tool", BOLD), color(&tool.name, GREEN)));
    lines.push(format!("{}: {}", color("Server", BOLD), color(server_name, CYAN)));
    lines.push(String::new());

    if let Some(ref desc) = tool.description {
        lines.push(color("Description:", BOLD));
        lines.push(format!("  {}", desc));
        lines.push(String::new());
    }

    lines.push(color("Input Schema:", BOLD));
    lines.push(serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default());

    lines.join("\n")
}

pub fn format_tool_result(result: &serde_json::Value) -> String {
    // Replicates TS formatToolResult:
    // Extract text content from MCP content array if present
    if let Some(obj) = result.as_object() {
        if let Some(content) = obj.get("content").and_then(|c| c.as_array()) {
            let text_parts: Vec<&str> = content
                .iter()
                .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect();

            if !text_parts.is_empty() {
                return text_parts.join("\n");
            }
        }
    }

    serde_json::to_string_pretty(result).unwrap_or_default()
}

pub fn format_json(data: &serde_json::Value) -> String {
    serde_json::to_string_pretty(data).unwrap_or_default()
}

pub fn format_error(message: &str) -> String {
    color(&format!("Error: {}", message), "\x1b[31m")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_format_tool_result_with_text() {
        let payload = json!({
            "content": [
                {
                    "type": "text",
                    "text": "Hello world from tool"
                }
            ]
        });
        let formatted = format_tool_result(&payload);
        assert_eq!(formatted, "Hello world from tool");
    }

    #[test]
    fn test_format_tool_result_raw_json() {
        let payload = json!({
            "some_key": "some_value"
        });
        let formatted = format_tool_result(&payload);
        assert!(formatted.contains("\"some_key\": \"some_value\""));
    }

    #[test]
    fn test_format_error() {
        // Since test env might have different terminal flags, check result contains "Error: "
        let formatted = format_error("Something went wrong");
        assert!(formatted.contains("Error: Something went wrong"));
    }
}
