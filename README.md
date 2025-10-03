# Operion Sentinel Agent

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Release](https://img.shields.io/github/v/release/grahamsutton/sentinel-agent)](https://github.com/grahamsutton/sentinel-agent/releases)

A lightweight, open-source monitoring agent for collecting system metrics and sending them to the Operion platform. Built in Rust for performance, reliability, and minimal resource usage.

## Features

- 🚀 **Lightweight**: Minimal resource footprint with Rust's zero-cost abstractions
- 📊 **Disk Monitoring**: Comprehensive disk space usage tracking with filtering
- 🔧 **YAML Configuration**: Flexible, human-readable configuration 
- 📦 **Batched Collection**: Efficient metric batching and HTTP API delivery
- 🛡️ **Secure**: No proprietary secrets, fully auditable open source
- 🔄 **Auto-restart**: Systemd service with automatic restart on failure
- 🎯 **DataDog-style**: Familiar configuration patterns for easy adoption

## Quick Start

### One-Line Installation

```bash
# Install latest version
curl -fsSL https://github.com/grahamsutton/sentinel-agent/releases/latest/download/install.sh | bash

# Install specific version
curl -fsSL https://github.com/grahamsutton/sentinel-agent/releases/download/v1.0.0/install.sh | bash
```

### Manual Installation

1. Download the binary for your platform from [releases](https://github.com/grahamsutton/sentinel-agent/releases)
2. Create a configuration file (see [Configuration](#configuration))
3. Run the agent: `./sentinel-agent --config /path/to/config.yaml`

## Configuration

The agent uses YAML configuration files. Here's a complete example:

```yaml
# Operion Sentinel Agent Configuration

agent:
  # Unique identifier for this agent/server
  id: "web-server-01"
  # Optional: Override hostname detection
  hostname: "web01.example.com"

api:
  # REST API endpoint for metric ingestion
  endpoint: "https://api.operion.co"
  # Optional: Request timeout in seconds (default: 30)
  timeout_seconds: 30
  
  # API key for Operion platform authentication
  # Required for server registration and billing tracking
  # Get your API key from https://app.operion.co/settings/api-keys
  api_key: "your-api-key-here"

collection:
  # How often to collect metrics (seconds)
  interval_seconds: 60
  # How often to flush buffered metrics to API (seconds) 
  flush_interval_seconds: 10
  # Maximum metrics to buffer before dropping old ones
  batch_size: 100
  
  # Disk monitoring configuration
  disk:
    enabled: true
    # Optional: Only monitor these mount points (contains match)
    include_mount_points:
      - "/"
      - "/home"
    # Optional: Exclude these mount points (contains match)
    exclude_mount_points:
      - "/dev"
      - "/proc"
      - "/sys"
      - "/run"
      - "/tmp"
```

### System Installation

For system-wide installation (when run as root):
- Config: `/etc/operion/agent.yaml`
- Binary: `/usr/local/bin/sentinel-agent`
- Service: `systemctl status operion-agent`

### User Installation

For user installation (when run as regular user):
- Config: `~/.config/operion/agent.yaml`
- Binary: `~/.local/bin/sentinel-agent`

## Usage

### Command Line Options

```bash
# Use auto-detected config location
sentinel-agent

# Specify custom config file
sentinel-agent --config /path/to/config.yaml

# Show help and config locations
sentinel-agent --help
```

The agent automatically detects configuration files in this order:
1. `~/.config/operion/agent.yaml` (user installation)
2. `~/Library/Application Support/operion/agent.yaml` (macOS fallback)
3. `/etc/operion/agent.yaml` (system installation)
4. `./agent.yaml` (development)

### Systemd Service Management

```bash
# Start the service
sudo systemctl start operion-agent

# Enable auto-start on boot
sudo systemctl enable operion-agent

# Check status
sudo systemctl status operion-agent

# View logs
sudo journalctl -u operion-agent -f
```

## Metrics Collected

### Disk Metrics

- **Device**: Disk device name
- **Mount Point**: Filesystem mount point
- **Total Space**: Total disk space in bytes
- **Used Space**: Used disk space in bytes  
- **Available Space**: Available disk space in bytes
- **Usage Percentage**: Disk usage as decimal (0.0 to 1.0)

All metrics include timestamps and are sent to your configured API endpoint in JSON format.

## Building from Source

### Prerequisites

- Rust 1.70 or later
- Cargo package manager

### Build Steps

```bash
# Clone the repository
git clone https://github.com/grahamsutton/sentinel-agent.git
cd sentinel-agent

# Build release binary
cargo build --release

# Binary will be at target/release/sentinel-agent
```

### Cross-compilation

The project supports cross-compilation for multiple platforms:

```bash
# Install target for ARM64 Linux
rustup target add aarch64-unknown-linux-gnu

# Cross-compile
cargo build --target aarch64-unknown-linux-gnu --release
```

## API Integration

The agent sends metrics via HTTP POST to `/api/v1/metrics` with the following JSON structure:

```json
{
  "resource_id": "res_abc123def456",
  "hostname": "web01.example.com", 
  "timestamp": 1640995200,
  "metrics": [
    {
      "timestamp": 1640995200,
      "device": "/dev/sda1",
      "mount_point": "/",
      "total_space_bytes": 100000000000,
      "used_space_bytes": 50000000000,
      "available_space_bytes": 50000000000,
      "usage_percentage": 0.50
    }
  ]
}
```

## Security & Privacy

- ✅ **Open Source**: Full source code available for audit
- ✅ **No Secrets**: No proprietary code or hidden functionality
- ✅ **Minimal Data**: Only collects essential system metrics
- ✅ **Configurable**: Full control over what data is collected
- ✅ **Transparent**: Clear API payload structure

## Development & Testing

### Prerequisites

- Rust 1.70+
- Docker and Docker Compose
- make (optional, for convenience)

### Development Setup

```bash
# Clone and setup
git clone https://github.com/grahamsutton/sentinel-agent.git
cd sentinel-agent

# Build the agent
make build

# Run all tests (unit + integration)
make test

# Unit tests only
make test-unit

# Integration tests with Docker
make test-integration

# Generate coverage report
make coverage
```

### Integration Testing

The project includes comprehensive Docker-based integration tests that simulate real-world deployment:

```bash
# Run the full integration test suite
./tests/integration/run_integration_test.sh
```

The integration test:
- Builds the agent binary
- Creates Docker images for agent and mock API server
- Starts both services with docker-compose
- Waits for metrics to be collected and sent
- Validates metrics structure and content
- Reports success/failure with detailed logs

### CI/CD Pipeline

GitHub Actions workflows provide:
- **Unit Tests**: Run on every push/PR
- **Integration Tests**: Full Docker-based testing
- **Build Artifacts**: Release binaries for multiple platforms

## Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for your changes
4. Ensure all tests pass: `make test`
5. Submit a pull request

## Releases

### Creating a Release

Releases are automated via GitHub Actions when version tags are pushed:

```bash
# Create and push a new release
make release VERSION=v1.0.0
```

This will:
1. Run all tests (unit + integration) 
2. Create and push a git tag
3. Trigger GitHub Actions to build cross-platform binaries
4. Test each binary on its target architecture
5. Create a GitHub release with automated release notes
6. Generate checksums for security verification

### Release Process

1. **Manual Decision**: Determine version number using semantic versioning
2. **Automated Execution**: GitHub Actions handles testing, building, and publishing
3. **Cross-Platform**: Builds for Linux (x86_64, ARM64), macOS (Intel, Apple Silicon), and Windows
4. **Binary Validation**: Each binary is tested on its target architecture using native runners and QEMU
5. **Security**: Includes SHA256 checksums for all binaries

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Support

- 📖 **Documentation**: [docs.operion.co](https://docs.operion.co)
- 🐛 **Issues**: [GitHub Issues](https://github.com/grahamsutton/sentinel-agent/issues)
- 💬 **Community**: [Discord](https://discord.gg/operion)
- 📧 **Email**: support@operion.co

---

Made with ❤️ by the Operion team. Building transparent, trustworthy monitoring tools.