/**
 * Info command - Show server or tool details
 */

import { connectToServer, listTools, getTool } from '../client.js';
import { loadConfig, getServerConfig, type McpServersConfig } from '../config.js';
import {
  formatServerDetails,
  formatToolSchema,
  formatJson,
} from '../output.js';
import {
  formatCliError,
  serverConnectionError,
  toolNotFoundError,
  ErrorCode,
} from '../errors.js';

export interface InfoOptions {
  target: string; // "server" or "server/tool"
  json: boolean;
  withDescriptions: boolean;
  configPath?: string;
}

/**
 * Parse target into server and optional tool name
 */
function parseTarget(target: string): { server: string; tool?: string } {
  const parts = target.split('/');
  if (parts.length === 1) {
    return { server: parts[0] };
  }
  return { server: parts[0], tool: parts.slice(1).join('/') };
}

/**
 * Execute the info command
 */
export async function infoCommand(options: InfoOptions): Promise<void> {
  let config: McpServersConfig;

  try {
    config = await loadConfig(options.configPath);
  } catch (error) {
    console.error((error as Error).message);
    process.exit(ErrorCode.CLIENT_ERROR);
  }

  const { server: serverName, tool: toolName } = parseTarget(options.target);

  let serverConfig;
  try {
    serverConfig = getServerConfig(config, serverName);
  } catch (error) {
    console.error((error as Error).message);
    process.exit(ErrorCode.CLIENT_ERROR);
  }

  let client;
  let close: () => Promise<void>;

  try {
    const connection = await connectToServer(serverName, serverConfig);
    client = connection.client;
    close = connection.close;
  } catch (error) {
    console.error(formatCliError(serverConnectionError(serverName, (error as Error).message)));
    process.exit(ErrorCode.NETWORK_ERROR);
  }

  try {
    if (toolName) {
      // Show specific tool schema
      const tools = await listTools(client);
      const tool = tools.find(t => t.name === toolName);

      if (!tool) {
        const availableTools = tools.map(t => t.name);
        console.error(formatCliError(toolNotFoundError(toolName, serverName, availableTools)));
        process.exit(ErrorCode.CLIENT_ERROR);
      }

      if (options.json) {
        console.log(formatJson({
          name: tool.name,
          description: tool.description,
          inputSchema: tool.inputSchema,
        }));
      } else {
        console.log(formatToolSchema(serverName, tool));
      }
    } else {
      // Show server details
      const tools = await listTools(client);

      if (options.json) {
        console.log(formatJson({
          name: serverName,
          config: serverConfig,
          tools: tools.map((t) => ({
            name: t.name,
            description: t.description,
            inputSchema: t.inputSchema,
          })),
        }));
      } else {
        console.log(formatServerDetails(serverName, serverConfig, tools, options.withDescriptions));
      }
    }
  } finally {
    await close();
  }
}
