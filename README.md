# mcp-cli

A lightweight, Bun-based CLI for interacting with [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) servers.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- 🪶 **Lightweight** - Minimal dependencies, fast startup
- 📦 **Single Binary** - Compile to standalone executable via `bun build --compile`
- 🔧 **Shell-Friendly** - JSON output for scripting, intuitive commands
- 🤖 **Agent-Optimized** - Designed for AI coding agents (Gemini CLI, Claude Code, etc.)
- 🔌 **Universal** - Supports both stdio and HTTP MCP servers
- 💡 **Actionable Errors** - Structured error messages with recovery suggestions

## Installation

### Via Bun (recommended)

```bash
bun install -g mcp-cli
```

### Via npm

```bash
npm install -g mcp-cli
```

### From Source

```bash
git clone https://github.com/philschmid/mcp-cli
cd mcp-cli
bun install
bun run build
```

### Pre-built Binaries

Install the latest release with the install script (auto-detects OS/architecture):

```bash
curl -fsSL https://raw.githubusercontent.com/philschmid/mcp-cli/main/install.sh | bash
```

Or download manually from the [releases page](https://github.com/philschmid/mcp-cli/releases).

## Quick Start

### 1. Create a config file

Create `mcp_servers.json` in your current directory or `~/.config/mcp/`:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_TOKEN": "${GITHUB_TOKEN}"
      }
    }
  }
}
```

### 2. Discover available tools

```bash
# List all servers and tools
mcp-cli

# With descriptions
mcp-cli -d
```

### 3. Call a tool

```bash
# View tool schema first
mcp-cli filesystem/read_file

# Call the tool
mcp-cli filesystem/read_file '{"path": "./README.md"}'
```

## Usage

```
mcp-cli [options]                           List all servers and tools (names only)
mcp-cli [options] grep <pattern>            Search tools by glob pattern
mcp-cli [options] <server>                  Show server tools and parameters
mcp-cli [options] <server>/<tool>           Show tool schema (JSON input schema)
mcp-cli [options] <server>/<tool> <json>    Call tool with arguments
```

**Tip:** Add `-d` to any command to include descriptions.

### Options

| Option | Description |
|--------|-------------|
| `-h, --help` | Show help message |
| `-v, --version` | Show version number |
| `-j, --json` | Output as JSON (for scripting) |
| `-r, --raw` | Output raw text content |
| `-d, --with-descriptions` | Include tool descriptions |
| `-c, --config <path>` | Path to config file |

### Output

| Stream | Content |
|--------|---------|
| **stdout** | Tool results and data (text by default, JSON with `--json`) |
| **stderr** | Errors and diagnostics |

### Commands

#### List Servers

```bash
# Basic listing
$ mcp-cli
github
  • search_repositories
  • get_file_contents
  • create_or_update_file
filesystem
  • read_file
  • write_file
  • list_directory

# With descriptions
$ mcp-cli --with-descriptions
github
  • search_repositories - Search for GitHub repositories
  • get_file_contents - Get contents of a file or directory
filesystem
  • read_file - Read the contents of a file
  • write_file - Write content to a file
```

#### Search Tools

```bash
# Find file-related tools across all servers
$ mcp-cli grep "*file*"
github/get_file_contents
github/create_or_update_file
filesystem/read_file
filesystem/write_file

# Search with descriptions
$ mcp-cli grep "*search*" -d
github/search_repositories - Search for GitHub repositories
```

#### View Server Details

```bash
$ mcp-cli github
Server: github
Transport: stdio
Command: npx -y @modelcontextprotocol/server-github

Tools (12):
  search_repositories
    Search for GitHub repositories
    Parameters:
      • query (string, required) - Search query
      • page (number, optional) - Page number
  ...
```

#### View Tool Schema

```bash
$ mcp-cli github/search_repositories
Tool: search_repositories
Server: github

Description:
  Search for GitHub repositories

Input Schema:
  {
    "type": "object",
    "properties": {
      "query": { "type": "string", "description": "Search query" },
      "page": { "type": "number" }
    },
    "required": ["query"]
  }
```

#### Call a Tool

```bash
# With inline JSON
$ mcp-cli github/search_repositories '{"query": "mcp server", "per_page": 5}'

# From stdin
$ echo '{"query": "mcp"}' | mcp-cli github/search_repositories

# JSON output for scripting
$ mcp-cli github/search_repositories '{"query": "mcp"}' --json | jq '.content[0].text'
```

## Configuration

### Config File Format

The CLI uses `mcp_servers.json`, compatible with Claude Desktop, Gemini or VS Code:

```json
{
  "mcpServers": {
    "local-server": {
      "command": "node",
      "args": ["./server.js"],
      "env": {
        "API_KEY": "${API_KEY}"
      },
      "cwd": "/path/to/directory"
    },
    "remote-server": {
      "url": "https://mcp.example.com",
      "headers": {
        "Authorization": "Bearer ${TOKEN}"
      }
    }
  }
}
```

### Config Resolution

The CLI searches for configuration in this order:

1. `MCP_CONFIG_PATH` environment variable
2. `-c/--config` command line argument
3. `./mcp_servers.json` (current directory)
4. `~/.mcp_servers.json`
5. `~/.config/mcp/mcp_servers.json`

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_CONFIG_PATH` | Path to config file | (none) |
| `MCP_DEBUG` | Enable debug output | `false` |
| `MCP_TIMEOUT` | Request timeout (seconds) | `30` |

## Using with AI Agents

`mcp-cli` is designed to give AI coding agents access to MCP (Model Context Protocol) servers. MCP enables AI models to interact with external tools, APIs, and data sources through a standardized protocol.

### Why MCP + CLI?

Traditional MCP integration loads full tool schemas into the AI's context window, consuming thousands of tokens. The CLI approach:

- **On-demand loading**: Only fetch schemas when needed
- **Token efficient**: Minimal context overhead
- **Shell composable**: Chain with `jq`, pipes, and scripts
- **Scriptable**: AI can write shell scripts for complex workflows

### Option 1: System Prompt Integration

Add this to your AI agent's system prompt for direct CLI access:

```xml
<mcp_tools>
You have access to MCP (Model Context Protocol) servers via the `mcp-cli` command.
MCP provides tools for interacting with external systems like GitHub, filesystems, databases, and APIs.

## Available Commands

```bash
mcp-cli                              # List all servers and tool names
mcp-cli <server>                     # Show server tools and parameters
mcp-cli <server>/<tool>              # Get tool JSON schema and descriptions
mcp-cli <server>/<tool> '<json>'     # Call tool with JSON arguments
mcp-cli grep "<pattern>"             # Search tools by name (glob pattern)
```

**Add `-d` to include tool descriptions** (e.g., `mcp-cli filesystem -d`)

## Workflow

1. **Discover**: Run `mcp-cli` to see available servers and tools or `mcp-cli grep "<pattern>"` to search for tools by name (glob pattern)
2. **Explore**: Run `mcp-cli <server>` to see all tools with descriptions and parameters
3. **Inspect**: Run `mcp-cli <server>/<tool>` to get the full JSON input schema if required context is missing. 
4. **Execute**: Run `mcp-cli <server>/<tool> '<json>'` with correct arguments

## Examples

```bash
# List available servers and tools
mcp-cli

# See all tools for a server (use -d to include description for tool and arguments)
mcp-cli filesystem

# Get JSON schema for a specific tool including description
mcp-cli filesystem/read_file

# Call the tool
mcp-cli filesystem/read_file '{"path": "./README.md"}'

# JSON output for parsing
mcp-cli filesystem/read_file '{"path": "./README.md"}' --json
```

## Rules

1. **Always check schema first**: Run `mcp-cli <server>/<tool>` before calling any tool
2. **Use --json for parsing**: Add `--json` when you need to process the output
3. **Quote JSON arguments**: Wrap JSON in single quotes to prevent shell interpretation
</mcp_tools>
```

### Option 2: Agents Skill

For Code Agents that support Agents Skills, like Gemini CLI, OpenCode or Claude Code. you can use the mcp-cli skill to interface with MCP servers. The Skill is available at [mcp-cli/SKILL.md](mcp-cli/SKILL.md)

Create `mcp-cli/SKILL.md` in your skills directory. 

## Development

### Prerequisites

- [Bun](https://bun.sh/) >= 1.0.0

### Setup

```bash
git clone https://github.com/philschmid/mcp-cli
cd mcp-cli
bun install
```

### Commands

```bash
# Run in development
bun run dev

# Type checking
bun run typecheck

# Linting
bun run lint
bun run lint:fix

# Run all tests (unit + integration)
bun test

# Run only unit tests (fast)
bun test tests/config.test.ts tests/output.test.ts tests/client.test.ts

# Run integration tests (requires MCP server, ~35s)
bun test tests/integration/

# Build single executable
bun run build

# Build for all platforms
bun run build:all
```

### Local Testing

Test the CLI locally without compiling by using `bun link`:

```bash
# Link the package globally (run once)
bun link

# Now you can use 'mcp-cli' anywhere
mcp-cli --help
mcp-cli filesystem/read_file '{"path": "./README.md"}'

# Or run directly during development
bun run dev --help
bun run dev filesystem
```

To unlink when done:

```bash
bun unlink mcp-cli
```

### Releasing

Releases are automated via GitHub Actions. Use the release script:

```bash
./scripts/release.sh 0.2.0
```

This script will:
1. Update version in `package.json` and `src/index.ts`
2. Run type checking, linting, and tests
3. Commit the version bump
4. Create and push a git tag

The GitHub Actions release workflow then:
1. Builds binaries for Linux x64, macOS x64, and macOS ARM64
2. Creates a GitHub release with auto-generated notes
3. Attaches all binaries to the release

### Error Messages

All errors include actionable recovery suggestions, optimized for both humans and AI agents:

```
Error [CONFIG_NOT_FOUND]: Config file not found: /path/config.json
  Suggestion: Create mcp_servers.json with: { "mcpServers": { "server-name": { "command": "..." } } }

Error [SERVER_NOT_FOUND]: Server "github" not found in config
  Details: Available servers: filesystem, sqlite
  Suggestion: Use one of: mcp-cli filesystem, mcp-cli sqlite

Error [INVALID_JSON_ARGUMENTS]: Invalid JSON in tool arguments
  Details: Parse error: Unexpected identifier "test"
  Suggestion: Arguments must be valid JSON. Use single quotes: '{"key": "value"}'

Error [TOOL_NOT_FOUND]: Tool "search" not found in server "filesystem"
  Details: Available tools: read_file, write_file, list_directory (+5 more)
  Suggestion: Run 'mcp-cli filesystem' to see all available tools
```

## Roadmap

### Current (v0.1)

- [x] List servers and tools
- [x] Search with glob patterns
- [x] View server/tool details
- [x] Call tools with JSON arguments
- [x] stdio and HTTP transport support
- [x] JSON output mode for scripting
- [x] Environment variable substitution
- [x] Retry logic for transient failures

### Future Considerations

- [ ] **Resources** - `mcp-cli resources <server>` to list/read MCP resources
- [ ] **Prompts** - `mcp-cli prompts <server>` to list/get MCP prompts
- [ ] **Sessions** - Persistent connections for repeated calls
- [ ] **OAuth Support** - Full OAuth 2.1 flow for remote servers
- [ ] **Proxy Mode** - Expose authenticated sessions via local proxy
- [ ] **Config Merging** - Merge configs from multiple locations

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request
