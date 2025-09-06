# Test Runner MCP server

A configurable test runner MCP (Model Context Protocol) server that allows you to run tests with a customizable command.

## Features

- Configurable RSpec command via CLI argument
- MCP server over HTTP with Server-Sent Events (SSE)
- Supports any RSpec command (local, Docker-based, or custom)

## Usage

Start the server with a custom RSpec command:

```bash
# Using default command (bundle exec rspec)
./test-runner-mcp

# Using local rspec
./test-runner-mcp --rspec-command "rspec"

# Using Docker compose
./test-runner-mcp --rspec-command "docker compose exec test bundle exec rspec"

# Custom hostname and port
./test-runner-mcp --hostname 0.0.0.0 --port 3030 --rspec-command "bundle exec rspec"
```

## CLI Options

- `-H, --hostname <HOSTNAME>` - Server hostname (default: 127.0.0.1)
- `-p, --port <PORT>` - Server port (default: 30301)
- `-c, --rspec-command <RSPEC_COMMAND>` - RSpec command to execute (default: "bundle exec rspec")

## MCP Tool

The server exposes one tool:

- `run_rspec` - Run tests for a specified file using the configured command

## API Endpoints

- `GET /sse` - Server-Sent Events endpoint for MCP communication
- `POST /message` - Message endpoint for MCP requests
