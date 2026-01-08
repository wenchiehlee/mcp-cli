/**
 * Grep command - Search tools by pattern
 */

import { connectToServer, listTools, type ToolInfo } from '../client.js';
import {
  loadConfig,
  getServerConfig,
  listServerNames,
  type McpServersConfig,
} from '../config.js';
import { formatSearchResults, formatJson } from '../output.js';
import { ErrorCode } from '../errors.js';

export interface GrepOptions {
  pattern: string;
  withDescriptions: boolean;
  json: boolean;
  configPath?: string;
}

interface SearchResult {
  server: string;
  tool: ToolInfo;
}

/**
 * Convert glob pattern to regex
 */
function globToRegex(pattern: string): RegExp {
  const escaped = pattern
    .replace(/[.+^${}()|[\]\\]/g, '\\$&') // Escape special regex chars
    .replace(/\*/g, '.*') // * -> .*
    .replace(/\?/g, '.'); // ? -> .

  return new RegExp(`^${escaped}$`, 'i');
}

/**
 * Execute the grep command
 */
export async function grepCommand(options: GrepOptions): Promise<void> {
  let config: McpServersConfig;

  try {
    config = await loadConfig(options.configPath);
  } catch (error) {
    console.error((error as Error).message);
    process.exit(ErrorCode.CLIENT_ERROR);
  }

  const pattern = globToRegex(options.pattern);
  const serverNames = listServerNames(config);
  const results: SearchResult[] = [];

  for (const serverName of serverNames) {
    try {
      const serverConfig = getServerConfig(config, serverName);
      const { client, close } = await connectToServer(serverName, serverConfig);

      try {
        const tools = await listTools(client);

        for (const tool of tools) {
          // Match against tool name, server/tool path, or description
          const fullPath = `${serverName}/${tool.name}`;
          const matchesName = pattern.test(tool.name);
          const matchesPath = pattern.test(fullPath);
          const matchesDescription =
            tool.description && pattern.test(tool.description);

          if (matchesName || matchesPath || matchesDescription) {
            results.push({ server: serverName, tool });
          }
        }
      } finally {
        await close();
      }
    } catch (error) {
      // Skip servers that fail to connect
      if (process.env.MCP_DEBUG) {
        console.error(
          `Warning: Failed to connect to ${serverName}: ${(error as Error).message}`
        );
      }
    }
  }

  if (results.length === 0) {
    console.log(`No tools found matching "${options.pattern}"`);
    return;
  }

  if (options.json) {
    const jsonOutput = results.map((r) => ({
      server: r.server,
      tool: r.tool.name,
      description: r.tool.description,
      inputSchema: r.tool.inputSchema,
    }));
    console.log(formatJson(jsonOutput));
  } else {
    console.log(formatSearchResults(results, options.withDescriptions));
  }
}
