/**
 * MCP-CLI Configuration Types and Loader
 */

import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  configNotFoundError,
  configSearchError,
  configInvalidJsonError,
  configMissingFieldError,
  serverNotFoundError,
  formatCliError,
} from './errors.js';

/**
 * stdio server configuration (local process)
 */
export interface StdioServerConfig {
  command: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
}

/**
 * HTTP server configuration (remote)
 */
export interface HttpServerConfig {
  url: string;
  headers?: Record<string, string>;
  timeout?: number;
}

export type ServerConfig = StdioServerConfig | HttpServerConfig;

export interface McpServersConfig {
  mcpServers: Record<string, ServerConfig>;
}

/**
 * Check if a server config is HTTP-based
 */
export function isHttpServer(config: ServerConfig): config is HttpServerConfig {
  return 'url' in config;
}

/**
 * Check if a server config is stdio-based
 */
export function isStdioServer(
  config: ServerConfig
): config is StdioServerConfig {
  return 'command' in config;
}

/**
 * Substitute environment variables in a string
 * Supports ${VAR_NAME} syntax
 */
function substituteEnvVars(value: string): string {
  return value.replace(/\$\{([^}]+)\}/g, (_, varName) => {
    return process.env[varName] || '';
  });
}

/**
 * Recursively substitute environment variables in an object
 */
function substituteEnvVarsInObject<T>(obj: T): T {
  if (typeof obj === 'string') {
    return substituteEnvVars(obj) as T;
  }
  if (Array.isArray(obj)) {
    return obj.map(substituteEnvVarsInObject) as T;
  }
  if (obj && typeof obj === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      result[key] = substituteEnvVarsInObject(value);
    }
    return result as T;
  }
  return obj;
}

/**
 * Get default config search paths
 */
function getDefaultConfigPaths(): string[] {
  const paths: string[] = [];
  const home = homedir();

  // Current directory
  paths.push(resolve('./mcp_servers.json'));

  // Home directory variants
  paths.push(join(home, '.mcp_servers.json'));
  paths.push(join(home, '.config', 'mcp', 'mcp_servers.json'));

  return paths;
}

/**
 * Load and parse MCP servers configuration
 */
export async function loadConfig(
  explicitPath?: string
): Promise<McpServersConfig> {
  let configPath: string | undefined;

  // Check explicit path from argument or environment
  if (explicitPath) {
    configPath = resolve(explicitPath);
  } else if (process.env.MCP_CONFIG_PATH) {
    configPath = resolve(process.env.MCP_CONFIG_PATH);
  }

  // If explicit path provided, it must exist
  if (configPath) {
    if (!existsSync(configPath)) {
      throw new Error(formatCliError(configNotFoundError(configPath)));
    }
  } else {
    // Search default paths
    const searchPaths = getDefaultConfigPaths();
    for (const path of searchPaths) {
      if (existsSync(path)) {
        configPath = path;
        break;
      }
    }

    if (!configPath) {
      throw new Error(formatCliError(configSearchError()));
    }
  }

  // Read and parse config
  const file = Bun.file(configPath);
  const content = await file.text();

  let config: McpServersConfig;
  try {
    config = JSON.parse(content);
  } catch (e) {
    throw new Error(formatCliError(configInvalidJsonError(configPath, (e as Error).message)));
  }

  // Validate structure
  if (!config.mcpServers || typeof config.mcpServers !== 'object') {
    throw new Error(formatCliError(configMissingFieldError(configPath)));
  }

  // Substitute environment variables
  config = substituteEnvVarsInObject(config);

  return config;
}

/**
 * Get a specific server config by name
 */
export function getServerConfig(
  config: McpServersConfig,
  serverName: string
): ServerConfig {
  const server = config.mcpServers[serverName];
  if (!server) {
    const available = Object.keys(config.mcpServers);
    throw new Error(formatCliError(serverNotFoundError(serverName, available)));
  }
  return server;
}

/**
 * List all server names
 */
export function listServerNames(config: McpServersConfig): string[] {
  return Object.keys(config.mcpServers);
}
