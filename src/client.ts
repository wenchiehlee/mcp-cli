/**
 * MCP Client - Connection management for MCP servers
 */

import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js';
import {
  type HttpServerConfig,
  type ServerConfig,
  type StdioServerConfig,
  isHttpServer,
} from './config.js';
import { VERSION } from './version.js';

export interface ConnectedClient {
  client: Client;
  close: () => Promise<void>;
}

export interface ServerInfo {
  name: string;
  version?: string;
  protocolVersion?: string;
}

export interface ToolInfo {
  name: string;
  description?: string;
  inputSchema: Record<string, unknown>;
}

/**
 * Retry configuration
 */
interface RetryConfig {
  maxRetries: number;
  baseDelayMs: number;
  maxDelayMs: number;
}

const DEFAULT_RETRY_CONFIG: RetryConfig = {
  maxRetries: 3,
  baseDelayMs: 1000,
  maxDelayMs: 10000,
};

/**
 * Check if an error is transient and worth retrying
 */
function isTransientError(error: Error): boolean {
  const message = error.message.toLowerCase();

  // Network errors
  if (message.includes('econnrefused')) return true;
  if (message.includes('econnreset')) return true;
  if (message.includes('etimedout')) return true;
  if (message.includes('enotfound')) return true;
  if (message.includes('epipe')) return true;
  if (message.includes('network')) return true;

  // HTTP transient errors
  if (message.includes('502')) return true; // Bad Gateway
  if (message.includes('503')) return true; // Service Unavailable
  if (message.includes('504')) return true; // Gateway Timeout
  if (message.includes('429')) return true; // Too Many Requests

  // Connection issues
  if (message.includes('connection')) return true;
  if (message.includes('timeout')) return true;

  return false;
}

/**
 * Calculate delay with exponential backoff and jitter
 */
function calculateDelay(attempt: number, config: RetryConfig): number {
  const exponentialDelay = config.baseDelayMs * Math.pow(2, attempt);
  const cappedDelay = Math.min(exponentialDelay, config.maxDelayMs);
  // Add jitter (±25%)
  const jitter = cappedDelay * 0.25 * (Math.random() * 2 - 1);
  return Math.round(cappedDelay + jitter);
}

/**
 * Sleep for specified milliseconds
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Execute a function with retry logic for transient failures
 */
async function withRetry<T>(
  fn: () => Promise<T>,
  operationName: string,
  config: RetryConfig = DEFAULT_RETRY_CONFIG
): Promise<T> {
  let lastError: Error | undefined;

  for (let attempt = 0; attempt <= config.maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error as Error;

      if (attempt < config.maxRetries && isTransientError(lastError)) {
        const delay = calculateDelay(attempt, config);
        if (process.env.MCP_DEBUG) {
          console.error(
            `[retry] ${operationName} failed (attempt ${attempt + 1}/${config.maxRetries + 1}): ${lastError.message}. Retrying in ${delay}ms...`
          );
        }
        await sleep(delay);
      } else {
        throw lastError;
      }
    }
  }

  throw lastError;
}

/**
 * Connect to an MCP server with retry logic
 */
export async function connectToServer(
  serverName: string,
  config: ServerConfig
): Promise<ConnectedClient> {
  return withRetry(
    async () => {
      const client = new Client(
        {
          name: 'mcp-cli',
          version: VERSION,
        },
        {
          capabilities: {},
        }
      );

      let transport: StdioClientTransport | StreamableHTTPClientTransport;

      if (isHttpServer(config)) {
        transport = createHttpTransport(config);
      } else {
        transport = createStdioTransport(config);
      }

      await client.connect(transport);

      return {
        client,
        close: async () => {
          await client.close();
        },
      };
    },
    `connect to ${serverName}`
  );
}

/**
 * Create HTTP transport for remote servers
 */
function createHttpTransport(
  config: HttpServerConfig
): StreamableHTTPClientTransport {
  const url = new URL(config.url);

  return new StreamableHTTPClientTransport(url, {
    requestInit: {
      headers: config.headers,
    },
  });
}

/**
 * Create stdio transport for local servers
 */
function createStdioTransport(config: StdioServerConfig): StdioClientTransport {
  // Merge process.env with config.env, filtering out undefined values
  const mergedEnv: Record<string, string> = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined) {
      mergedEnv[key] = value;
    }
  }
  if (config.env) {
    Object.assign(mergedEnv, config.env);
  }

  return new StdioClientTransport({
    command: config.command,
    args: config.args,
    env: mergedEnv,
    cwd: config.cwd,
  });
}

/**
 * List all tools from a connected client with retry logic
 */
export async function listTools(client: Client): Promise<ToolInfo[]> {
  return withRetry(
    async () => {
      const result = await client.listTools();
      return result.tools.map((tool: Tool) => ({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema as Record<string, unknown>,
      }));
    },
    'list tools'
  );
}

/**
 * Get a specific tool by name
 */
export async function getTool(
  client: Client,
  toolName: string
): Promise<ToolInfo | undefined> {
  const tools = await listTools(client);
  return tools.find((t) => t.name === toolName);
}

/**
 * Call a tool with arguments and retry logic
 */
export async function callTool(
  client: Client,
  toolName: string,
  args: Record<string, unknown>
): Promise<unknown> {
  return withRetry(
    async () => {
      const result = await client.callTool({
        name: toolName,
        arguments: args,
      });
      return result;
    },
    `call tool ${toolName}`
  );
}
