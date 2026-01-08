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
  debug,
  getConcurrencyLimit,
  getMaxRetries,
  getRetryDelayMs,
  getTimeoutMs,
  isHttpServer,
} from './config.js';
import { VERSION } from './version.js';

// Re-export config utilities for convenience
export { debug, getTimeoutMs, getConcurrencyLimit };

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
  totalBudgetMs: number;
}

/**
 * Get retry config respecting MCP_TIMEOUT budget
 */
function getRetryConfig(): RetryConfig {
  const totalBudgetMs = getTimeoutMs();
  const maxRetries = getMaxRetries();
  const baseDelayMs = getRetryDelayMs();

  // Reserve at least 5s for the final attempt
  const retryBudgetMs = Math.max(0, totalBudgetMs - 5000);

  return {
    maxRetries,
    baseDelayMs,
    maxDelayMs: Math.min(10000, retryBudgetMs / 2),
    totalBudgetMs,
  };
}

/**
 * Check if an error is transient and worth retrying
 * Uses error codes when available, falls back to message matching
 */
export function isTransientError(error: Error): boolean {
  // Check error code first (more reliable than message matching)
  const nodeError = error as NodeJS.ErrnoException;
  if (nodeError.code) {
    const transientCodes = [
      'ECONNREFUSED',
      'ECONNRESET',
      'ETIMEDOUT',
      'ENOTFOUND',
      'EPIPE',
      'ENETUNREACH',
      'EHOSTUNREACH',
      'EAI_AGAIN',
    ];
    if (transientCodes.includes(nodeError.code)) {
      return true;
    }
  }

  // Fallback to message matching for errors without codes
  const message = error.message;

  // HTTP transient errors - require status code at start or with HTTP context
  // Pattern: "502", "502 Bad Gateway", "HTTP 502", "status 502", "status code 502"
  if (/^(502|503|504|429)\b/.test(message)) return true;
  if (/\b(http|status(\s+code)?)\s*(502|503|504|429)\b/i.test(message))
    return true;
  if (
    /\b(502|503|504|429)\s+(bad gateway|service unavailable|gateway timeout|too many requests)/i.test(
      message,
    )
  )
    return true;

  // Generic network terms - more specific patterns
  if (/network\s*(error|fail|unavailable|timeout)/i.test(message)) return true;
  if (/connection\s*(reset|refused|timeout)/i.test(message)) return true;
  if (/\btimeout\b/i.test(message)) return true;

  return false;
}

/**
 * Calculate delay with exponential backoff and jitter
 */
function calculateDelay(attempt: number, config: RetryConfig): number {
  const exponentialDelay = config.baseDelayMs * 2 ** attempt;
  const cappedDelay = Math.min(exponentialDelay, config.maxDelayMs);
  // Add jitter (±25%)
  const jitter = cappedDelay * 0.25 * (Math.random() * 2 - 1);
  return Math.round(cappedDelay + jitter);
}

/**
 * Sleep for specified milliseconds
 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Execute a function with retry logic for transient failures
 * Respects overall timeout budget from MCP_TIMEOUT
 */
async function withRetry<T>(
  fn: () => Promise<T>,
  operationName: string,
  config: RetryConfig = getRetryConfig(),
): Promise<T> {
  let lastError: Error | undefined;
  const startTime = Date.now();

  for (let attempt = 0; attempt <= config.maxRetries; attempt++) {
    // Check if we've exceeded the total timeout budget
    const elapsed = Date.now() - startTime;
    if (elapsed >= config.totalBudgetMs) {
      debug(`${operationName}: timeout budget exhausted after ${elapsed}ms`);
      break;
    }

    try {
      return await fn();
    } catch (error) {
      lastError = error as Error;

      const remainingBudget = config.totalBudgetMs - (Date.now() - startTime);
      const shouldRetry =
        attempt < config.maxRetries &&
        isTransientError(lastError) &&
        remainingBudget > 1000; // At least 1s remaining

      if (shouldRetry) {
        const delay = Math.min(
          calculateDelay(attempt, config),
          remainingBudget - 1000,
        );
        debug(
          `${operationName} failed (attempt ${attempt + 1}/${config.maxRetries + 1}): ${lastError.message}. Retrying in ${delay}ms...`,
        );
        await sleep(delay);
      } else {
        throw lastError;
      }
    }
  }

  throw lastError;
}

/**
 * Safely close a connection, logging but not throwing on error
 */
export async function safeClose(close: () => Promise<void>): Promise<void> {
  try {
    await close();
  } catch (err) {
    debug(`Failed to close connection: ${(err as Error).message}`);
  }
}

/**
 * Connect to an MCP server with retry logic
 */
export async function connectToServer(
  serverName: string,
  config: ServerConfig,
): Promise<ConnectedClient> {
  return withRetry(async () => {
    const client = new Client(
      {
        name: 'mcp-cli',
        version: VERSION,
      },
      {
        capabilities: {},
      },
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
  }, `connect to ${serverName}`);
}

/**
 * Create HTTP transport for remote servers
 */
function createHttpTransport(
  config: HttpServerConfig,
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
  return withRetry(async () => {
    const result = await client.listTools();
    return result.tools.map((tool: Tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema as Record<string, unknown>,
    }));
  }, 'list tools');
}

/**
 * Get a specific tool by name
 */
export async function getTool(
  client: Client,
  toolName: string,
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
  args: Record<string, unknown>,
): Promise<unknown> {
  return withRetry(async () => {
    const result = await client.callTool({
      name: toolName,
      arguments: args,
    });
    return result;
  }, `call tool ${toolName}`);
}
