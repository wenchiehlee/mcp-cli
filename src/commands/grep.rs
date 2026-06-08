use crate::config::debug;
use crate::errors::CliError;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct GrepOptions {
    pub pattern: String,
    pub with_descriptions: bool,
    pub config_path: Option<String>,
}

pub struct SearchResult {
    pub server: String,
    pub tool: crate::output::ToolInfo,
}

struct ServerSearchResult {
    server_name: String,
    results: Vec<SearchResult>,
    error: Option<String>,
}

pub fn glob_to_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    let mut escaped = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let char = chars[i];

        if char == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            escaped.push_str(".*");
            i += 2;
            while i < chars.len() && chars[i] == '*' {
                i += 1;
            }
        } else if char == '*' {
            escaped.push_str("[^/]*");
            i += 1;
        } else if char == '?' {
            escaped.push_str("[^/]");
            i += 1;
        } else if "[.+^${}()|\\]".contains(char) {
            escaped.push('\\');
            escaped.push(char);
            i += 1;
        } else {
            escaped.push(char);
            i += 1;
        }
    }

    regex::RegexBuilder::new(&format!("^{}$", escaped))
        .case_insensitive(true)
        .build()
}

async fn search_server_tools(
    server_name: &str,
    config: &crate::config::McpServersConfig,
    pattern: &regex::Regex,
) -> ServerSearchResult {
    let server_config = match crate::config::get_server_config(config, server_name) {
        Ok(sc) => sc,
        Err(e) => {
            return ServerSearchResult {
                server_name: server_name.to_string(),
                results: Vec::new(),
                error: Some(e.message),
            };
        }
    };

    match crate::client::get_connection(server_name, &server_config).await {
        Ok(connection) => match connection.list_tools().await {
            Ok(tools) => {
                let mut results = Vec::new();
                for tool in tools {
                    if pattern.is_match(&tool.name) {
                        results.push(SearchResult {
                            server: server_name.to_string(),
                            tool,
                        });
                    }
                }
                let _ = connection.close().await;
                debug(&format!("{}: found {} matches", server_name, results.len()));
                ServerSearchResult {
                    server_name: server_name.to_string(),
                    results,
                    error: None,
                }
            }
            Err(e) => {
                let _ = connection.close().await;
                debug(&format!(
                    "{}: tools listing failed - {}",
                    server_name, e.message
                ));
                ServerSearchResult {
                    server_name: server_name.to_string(),
                    results: Vec::new(),
                    error: Some(e.message),
                }
            }
        },
        Err(e) => {
            debug(&format!(
                "{}: connection failed - {}",
                server_name, e.message
            ));
            ServerSearchResult {
                server_name: server_name.to_string(),
                results: Vec::new(),
                error: Some(e.message),
            }
        }
    }
}

pub async fn grep_command(options: GrepOptions) -> Result<(), CliError> {
    let config = crate::config::load_config(options.config_path.as_deref())?;

    let pattern = glob_to_regex(&options.pattern).map_err(|e| crate::errors::CliError {
        code: crate::errors::ErrorCode::ClientError,
        error_type: "INVALID_PATTERN".to_string(),
        message: format!("Invalid glob pattern: {}", e),
        details: None,
        suggestion: None,
    })?;

    let server_names = crate::config::list_server_names(&config);

    if server_names.is_empty() {
        eprintln!("Warning: No servers configured. Add servers to mcp_servers.json");
        return Ok(());
    }

    let concurrency_limit = crate::config::get_concurrency_limit();
    debug(&format!(
        "Searching {} servers for pattern \"{}\" (concurrency: {})",
        server_names.len(),
        options.pattern,
        concurrency_limit
    ));

    let semaphore = Arc::new(Semaphore::new(concurrency_limit));
    let mut tasks = Vec::new();

    for name in server_names {
        let name = name.clone();
        let config = config.clone();
        let pattern_clone = pattern.clone();
        let sem = Arc::clone(&semaphore);

        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            search_server_tools(&name, &config, &pattern_clone).await
        }));
    }

    let mut all_results = Vec::new();
    let mut failed_servers = Vec::new();

    for task in tasks {
        if let Ok(res) = task.await {
            all_results.extend(res.results);
            if let Some(_err) = res.error {
                failed_servers.push(res.server_name);
            }
        }
    }

    if !failed_servers.is_empty() {
        eprintln!(
            "Warning: {} server(s) failed to connect: {}",
            failed_servers.len(),
            failed_servers.join(", ")
        );
    }

    if all_results.is_empty() {
        println!("No tools found matching \"{}\"", options.pattern);
        println!("  Tip: Pattern matches tool names only (not server names)");
        println!("  Tip: Use '*' for wildcards, e.g. '*file*' or 'read_*'");
        println!("  Tip: Run 'mcp-cli' to list all available tools");
        return Ok(());
    }

    let display_results: Vec<crate::output::SearchResult> = all_results
        .into_iter()
        .map(|r| crate::output::SearchResult {
            server: r.server,
            tool: r.tool,
        })
        .collect();

    println!(
        "{}",
        crate::output::format_search_results(&display_results, options.with_descriptions)
    );

    Ok(())
}
