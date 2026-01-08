/**
 * Unit tests for MCP client module
 */

import { describe, test, expect } from 'bun:test';
import {
  type HttpServerConfig,
  type StdioServerConfig,
  isHttpServer,
  isStdioServer,
} from '../src/config';

describe('client', () => {
  describe('server config type guards', () => {
    test('identifies HTTP server configs', () => {
      const httpConfig: HttpServerConfig = {
        url: 'https://mcp.example.com',
        headers: { Authorization: 'Bearer token' },
      };

      expect(isHttpServer(httpConfig)).toBe(true);
      expect(isStdioServer(httpConfig)).toBe(false);
    });

    test('identifies stdio server configs', () => {
      const stdioConfig: StdioServerConfig = {
        command: 'node',
        args: ['./server.js'],
        env: { DEBUG: 'true' },
      };

      expect(isStdioServer(stdioConfig)).toBe(true);
      expect(isHttpServer(stdioConfig)).toBe(false);
    });

    test('handles minimal HTTP config', () => {
      const minimalHttp: HttpServerConfig = {
        url: 'https://example.com',
      };

      expect(isHttpServer(minimalHttp)).toBe(true);
    });

    test('handles minimal stdio config', () => {
      const minimalStdio: StdioServerConfig = {
        command: 'echo',
      };

      expect(isStdioServer(minimalStdio)).toBe(true);
    });
  });

  // Note: Actually testing connectToServer requires a real MCP server
  // Those tests are in the integration test suite
});
