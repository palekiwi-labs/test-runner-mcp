# Test Runner MCP server

A configurable test runner MCP (Model Context Protocol) server that allows you to run tests with a customizable command.

## Features

- Configurable RSpec command via CLI argument
- Configurable Cargo test command via CLI argument
- MCP server over HTTP with Server-Sent Events (SSE)
- Supports any RSpec command (local, Docker-based, or custom)
- Supports any Cargo test command (local, Docker-based, or custom)

## Usage

Start the server with a custom RSpec command:

```bash
# Using default command (bundle exec rspec)
./test-runner-mcp

# Using local rspec
./test-runner-mcp --rspec-command "rspec"

# Using Docker compose
./test-runner-mcp --rspec-command "docker compose exec test bundle exec rspec"

# Using custom cargo test command
./test-runner-mcp --cargo-command "cargo test --verbose"

# Custom hostname and port with both commands
./test-runner-mcp --hostname 0.0.0.0 --port 3030 --rspec-command "bundle exec rspec" --cargo-command "cargo test"
```

## Nix

### Development Environment
```bash
nix develop
```

### Building
```bash
nix build
```

### Running
```bash
# Run with nix (specify .#default explicitly to pass arguments)
nix run .#default -- --help
nix run .#default -- -H 0.0.0.0 -p 3030

# Or run the built binary directly
nix build
./result/bin/test-runner-mcp -H 0.0.0.0 -p 3030
```

## CLI Options

- `-H, --hostname <HOSTNAME>` - Server hostname (default: 127.0.0.1)
- `-p, --port <PORT>` - Server port (default: 30301)
- `-c, --rspec-command <RSPEC_COMMAND>` - RSpec command to execute (default: "bundle exec rspec")
- `-g, --cargo-command <CARGO_COMMAND>` - Cargo test command to execute (default: "cargo test")

## MCP Tools

The server exposes two tools:

- `run_rspec` - Run RSpec tests for a specified file using the configured command
- `cargo_test` - Run Cargo tests with optional pattern and arguments using the configured command

## API Endpoints

- `GET /sse` - Server-Sent Events endpoint for MCP communication
- `POST /message` - Message endpoint for MCP requests
