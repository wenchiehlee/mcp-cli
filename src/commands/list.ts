/**
 * List command - List all servers and their tools
 */

import { connectToServer, listTools, type ToolInfo } from '../client.js';
import {
  loadConfig,
  getServerConfig,
  listServerNames,
  type McpServersConfig,
} from '../config.js';
import { formatServerList, formatJson } from '../output.js';
import { ErrorCode, formatCliError, serverConnectionError } from '../errors.js';

export interface ListOptions {
  withDescriptions: boolean;
  json: boolean;
  configPath?: string;
}

interface ServerWithTools {
  name: string;
  tools: ToolInfo[];
}

/**
 * Execute the list command
 */
export async function listCommand(options: ListOptions): Promise<void> {
  let config: McpServersConfig;

  try {
    config = await loadConfig(options.configPath);
  } catch (error) {
    console.error((error as Error).message);
    process.exit(ErrorCode.CLIENT_ERROR);
  }

  const serverNames = listServerNames(config);
  const servers: ServerWithTools[] = [];

  for (const serverName of serverNames) {
    try {
      const serverConfig = getServerConfig(config, serverName);
      const { client, close } = await connectToServer(serverName, serverConfig);

      try {
        const tools = await listTools(client);
        servers.push({ name: serverName, tools });
      } finally {
        await close();
      }
    } catch (error) {
      // Include server with error message as a tool
      servers.push({
        name: serverName,
        tools: [
          {
            name: `<error: ${(error as Error).message}>`,
            description: undefined,
            inputSchema: {},
          },
        ],
      });
    }
  }

  if (options.json) {
    const jsonOutput = servers.map((s) => ({
      name: s.name,
      tools: s.tools.map((t) => ({
        name: t.name,
        description: t.description,
        inputSchema: t.inputSchema,
      })),
    }));
    console.log(formatJson(jsonOutput));
  } else {
    console.log(formatServerList(servers, options.withDescriptions));
  }
}
