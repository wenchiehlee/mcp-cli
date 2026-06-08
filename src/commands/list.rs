use crate::config::debug;
use crate::errors::CliError;
use crate::output::ToolInfo;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct ListOptions {
    pub with_descriptions: bool,
    pub config_path: Option<String>,
}

struct ServerWithTools {
    name: String,
    tools: Vec<ToolInfo>,
    instructions: Option<String>,
    error: Option<String>,
}

async fn fetch_server_tools(
    server_name: &str,
    config: &crate::config::McpServersConfig,
) -> ServerWithTools {
    let server_config = match crate::config::get_server_config(config, server_name) {
        Ok(sc) => sc,
        Err(e) => {
            return ServerWithTools {
                name: server_name.to_string(),
                tools: Vec::new(),
                instructions: None,
                error: Some(e.message),
            };
        }
    };

    match crate::client::get_connection(server_name, &server_config).await {
        Ok(connection) => {
            let tools_res = connection.list_tools().await;
            let inst_res = connection.get_instructions().await;

            let _ = connection.close().await;

            match tools_res {
                Ok(tools) => {
                    let instructions = inst_res.ok().flatten();
                    debug(&format!("{}: loaded {} tools", server_name, tools.len()));
                    ServerWithTools {
                        name: server_name.to_string(),
                        tools,
                        instructions,
                        error: None,
                    }
                }
                Err(e) => {
                    debug(&format!(
                        "{}: tools listing failed - {}",
                        server_name, e.message
                    ));
                    ServerWithTools {
                        name: server_name.to_string(),
                        tools: Vec::new(),
                        instructions: None,
                        error: Some(e.message),
                    }
                }
            }
        }
        Err(e) => {
            debug(&format!("{}: connection failed - {}", server_name, e.message));
            ServerWithTools {
                name: server_name.to_string(),
                tools: Vec::new(),
                instructions: None,
                error: Some(e.message),
            }
        }
    }
}

pub async fn list_command(options: ListOptions) -> Result<(), CliError> {
    let config = crate::config::load_config(options.config_path.as_deref())?;

    let server_names = crate::config::list_server_names(&config);

    if server_names.is_empty() {
        eprintln!("Warning: No servers configured. Add servers to mcp_servers.json");
        return Ok(());
    }

    let concurrency_limit = crate::config::get_concurrency_limit();
    debug(&format!(
        "Processing {} servers with concurrency {}",
        server_names.len(),
        concurrency_limit
    ));

    let semaphore = Arc::new(Semaphore::new(concurrency_limit));
    let mut tasks = Vec::new();

    for name in server_names {
        let name = name.clone();
        let config = config.clone();
        let sem = Arc::clone(&semaphore);

        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            fetch_server_tools(&name, &config).await
        }));
    }

    let mut servers = Vec::new();
    for task in tasks {
        if let Ok(res) = task.await {
            servers.push(res);
        }
    }

    servers.sort_by(|a, b| a.name.cmp(&b.name));

    let display_servers: Vec<crate::output::ServerInfo> = servers
        .into_iter()
        .map(|s| {
            let tools = if let Some(err) = s.error {
                vec![ToolInfo {
                    name: format!("<error: {}>", err),
                    description: None,
                    input_schema: serde_json::json!({}),
                }]
            } else {
                s.tools
            };

            crate::output::ServerInfo {
                name: s.name,
                instructions: s.instructions,
                tools,
            }
        })
        .collect();

    println!(
        "{}",
        crate::output::format_server_list(&display_servers, options.with_descriptions)
    );
    Ok(())
}
