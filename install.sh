#!/bin/bash

set -e

REPO="grahamsutton/sentinel-agent"
INSTALL_DIR="/usr/local/bin"
SERVICE_DIR="/etc/systemd/system"
CONFIG_DIR="/etc/operion"
VERSION=""

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --version)
            VERSION="$2"
            shift 2
            ;;
        -v)
            VERSION="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [--version VERSION]"
            echo ""
            echo "Options:"
            echo "  --version, -v    Specify version to install (e.g., v1.0.0)"
            echo "  --help, -h       Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                    # Install latest version"
            echo "  $0 --version v1.0.0   # Install specific version"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

echo "🛡️  Installing Operion Sentinel Agent..."

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64) ARCH="x86_64" ;;
    aarch64) ARCH="aarch64" ;;
    arm64) ARCH="aarch64" ;;
    *) echo "❌ Unsupported architecture: $ARCH" && exit 1 ;;
esac

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case $OS in
    linux) OS="unknown-linux-gnu" ;;
    darwin) OS="apple-darwin" ;;
    *) echo "❌ Unsupported OS: $OS" && exit 1 ;;
esac

TARGET="${ARCH}-${OS}"
BINARY_NAME="sentinel-agent"

# Get release version
if [ -z "$VERSION" ]; then
    echo "📡 Fetching latest release..."
    LATEST_RELEASE=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
    
    if [ -z "$LATEST_RELEASE" ]; then
        echo "❌ Failed to fetch latest release"
        exit 1
    fi
    
    VERSION="$LATEST_RELEASE"
    echo "📦 Found latest version: $VERSION"
else
    echo "📦 Installing specified version: $VERSION"
    
    # Validate that the specified version exists
    RELEASE_CHECK=$(curl -s "https://api.github.com/repos/${REPO}/releases/tags/${VERSION}" | grep '"tag_name"')
    if [ -z "$RELEASE_CHECK" ]; then
        echo "❌ Version $VERSION not found"
        echo "💡 Check available versions at: https://github.com/${REPO}/releases"
        exit 1
    fi
fi

# Map target to asset name
case $TARGET in
    x86_64-unknown-linux-gnu) ASSET_NAME="sentinel-agent-linux-x86_64" ;;
    aarch64-unknown-linux-gnu) ASSET_NAME="sentinel-agent-linux-aarch64" ;;
    x86_64-apple-darwin) ASSET_NAME="sentinel-agent-macos-x86_64" ;;
    aarch64-apple-darwin) ASSET_NAME="sentinel-agent-macos-aarch64" ;;
    *) echo "❌ Unsupported target: $TARGET" && exit 1 ;;
esac

# Download binary archive
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}.tar.gz"
TEMP_ARCHIVE="/tmp/${ASSET_NAME}.tar.gz"
TEMP_DIR="/tmp/sentinel-extract-$$"

echo "⬇️  Downloading ${ASSET_NAME}..."
curl -L -o "$TEMP_ARCHIVE" "$DOWNLOAD_URL"

if [ ! -f "$TEMP_ARCHIVE" ]; then
    echo "❌ Failed to download binary archive"
    exit 1
fi

# Extract binary
mkdir -p "$TEMP_DIR"
tar -xzf "$TEMP_ARCHIVE" -C "$TEMP_DIR"
TEMP_FILE="$TEMP_DIR/$BINARY_NAME"

if [ ! -f "$TEMP_FILE" ]; then
    echo "❌ Failed to extract binary"
    exit 1
fi

# Make executable
chmod +x "$TEMP_FILE"

# Check if running as root for system installation
if [ "$EUID" -eq 0 ]; then
    echo "🔧 Installing system-wide..."
    
    # Install binary
    mv "$TEMP_FILE" "${INSTALL_DIR}/${BINARY_NAME}"
    
    # Create config directory
    mkdir -p "$CONFIG_DIR"
    
    # Create systemd service
    cat > "${SERVICE_DIR}/operion-agent.service" << EOF
[Unit]
Description=Operion Sentinel Monitoring Agent
After=network.target
Wants=network.target

[Service]
Type=simple
User=operion
Group=operion
ExecStart=${INSTALL_DIR}/${BINARY_NAME} --config ${CONFIG_DIR}/agent.yaml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
WorkingDirectory=${CONFIG_DIR}

[Install]
WantedBy=multi-user.target
EOF

    # Create user for the service
    if ! id -u operion > /dev/null 2>&1; then
        useradd --system --no-create-home --shell /bin/false operion
        echo "👤 Created 'operion' system user"
    fi

    # Create YAML config file template
    cat > "${CONFIG_DIR}/agent.yaml" << EOF
# Operion Sentinel Agent Configuration

agent:
  # Unique identifier for this agent/server
  id: "server-$(hostname)"
  # Optional: Override hostname detection
  # hostname: "custom-hostname"

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
    # include_mount_points:
    #   - "/"
    #   - "/home"
    # Optional: Exclude these mount points (contains match)
    exclude_mount_points:
      - "/dev"
      - "/proc"
      - "/sys"
      - "/run"
      - "/tmp"
EOF

    chown operion:operion "${CONFIG_DIR}/agent.yaml"
    chmod 600 "${CONFIG_DIR}/agent.yaml"

    echo "✅ Installed successfully!"
    echo ""
    echo "📝 Next steps:"
    echo "1. Edit the configuration: sudo nano ${CONFIG_DIR}/agent.yaml"
    echo "2. Enable the service: sudo systemctl enable operion-agent"
    echo "3. Start the service: sudo systemctl start operion-agent"
    echo "4. Check status: sudo systemctl status operion-agent"
    
else
    echo "🔧 Installing for current user..."
    
    # Install to user's local bin
    USER_BIN="$HOME/.local/bin"
    USER_CONFIG="$HOME/.config/operion"
    mkdir -p "$USER_BIN"
    mkdir -p "$USER_CONFIG"
    mv "$TEMP_FILE" "${USER_BIN}/${BINARY_NAME}"
    
    # Create user config file template
    cat > "${USER_CONFIG}/agent.yaml" << EOF
# Operion Sentinel Agent Configuration

agent:
  # Unique identifier for this agent/server
  id: "user-$(hostname)"
  # Optional: Override hostname detection
  # hostname: "custom-hostname"

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
    # include_mount_points:
    #   - "/"
    #   - "/home"
    # Optional: Exclude these mount points (contains match)
    exclude_mount_points:
      - "/dev"
      - "/proc"
      - "/sys"
      - "/run"
      - "/tmp"
EOF
    
    echo "✅ Installed to ${USER_BIN}/${BINARY_NAME}"
    echo "📄 Created config at ${USER_CONFIG}/agent.yaml"
    echo ""
    echo "📝 Usage:"
    echo "${USER_BIN}/${BINARY_NAME} --config ${USER_CONFIG}/agent.yaml"
    echo ""
    echo "💡 Next steps:"
    echo "1. Edit the configuration: nano ${USER_CONFIG}/agent.yaml"
    echo "2. Run the agent: ${USER_BIN}/${BINARY_NAME} --config ${USER_CONFIG}/agent.yaml"
    echo ""
    echo "💡 Add ${USER_BIN} to your PATH if not already present:"
    echo "echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
fi

# Clean up temporary files
rm -f "$TEMP_ARCHIVE"
rm -rf "$TEMP_DIR"

echo ""
echo "🎉 Installation complete!"