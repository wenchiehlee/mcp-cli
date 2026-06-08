use crate::errors::{server_connection_error, tool_not_found_error, CliError};

pub struct InfoOptions {
    pub target: String, // "server" or "server/tool"
    pub with_descriptions: bool,
    pub config_path: Option<String>,
}

fn parse_target(target: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = target.split('/').collect();
    if parts.len() == 1 {
        (parts[0].to_string(), None)
    } else {
        (parts[0].to_string(), Some(parts[1..].join("/")))
    }
}

pub async fn info_command(options: InfoOptions) -> Result<(), CliError> {
    let config = crate::config::load_config(options.config_path.as_deref())?;

    let (server_name, tool_name) = parse_target(&options.target);

    let server_config = crate::config::get_server_config(&config, &server_name)?;

    let connection = crate::client::get_connection(&server_name, &server_config)
        .await
        .map_err(|e| server_connection_error(&server_name, &e.message))?;

    let tools = connection.list_tools().await?;

    if let Some(tool_name) = tool_name {
        // Show specific tool schema
        let tool = tools.iter().find(|t| t.name == tool_name);

        if tool.is_none() {
            let available_tools: Vec<String> = tools.into_iter().map(|t| t.name).collect();
            let err = tool_not_found_error(&tool_name, &server_name, Some(&available_tools));
            let _ = connection.close().await;
            return Err(err);
        }

        let tool = tool.unwrap();
        println!("{}", crate::output::format_tool_schema(&server_name, tool));
    } else {
        // Show server details
        let instructions = connection.get_instructions().await.ok().flatten();
        println!(
            "{}",
            crate::output::format_server_details(
                &server_name,
                &server_config,
                &tools,
                options.with_descriptions,
                instructions.as_deref()
            )
        );
    }

    let _ = connection.close().await;
    Ok(())
}
