# Sentinel Agent Makefile
# Provides convenient commands for development and testing

.PHONY: build test test-unit test-integration clean docker-build docker-test release help

# Default target
help:
	@echo "Sentinel Agent Development Commands"
	@echo "=================================="
	@echo "build              Build the agent binary"
	@echo "test               Run all tests"
	@echo "test-unit          Run unit tests only"
	@echo "test-integration   Run integration tests with Docker"
	@echo "docker-build       Build Docker images for testing"
	@echo "docker-test        Run integration tests in Docker"
	@echo "clean              Clean build artifacts"
	@echo "install            Install the agent locally"
	@echo "coverage           Generate test coverage report"
	@echo "release            Create a new release with cross-platform validation (requires VERSION)"

# Build the agent
build:
	@echo "🔨 Building Sentinel Agent..."
	cargo build --release

# Run all tests
test: test-unit test-integration

# Run unit tests
test-unit:
	@echo "🧪 Running unit tests..."
	cargo test --lib

# Run integration tests
test-integration:
	@echo "🚀 Running integration tests..."
	chmod +x tests/integration/run_integration_test.sh
	./tests/integration/run_integration_test.sh

# Build Docker images
docker-build:
	@echo "🐳 Building Docker images..."
	docker build -f tests/integration/Dockerfile.mock-api -t sentinel-mock-api .
	docker build -f tests/integration/Dockerfile.agent -t sentinel-agent .

# Run integration tests using Docker Compose
docker-test: docker-build
	@echo "🐳 Running Docker-based integration tests..."
	docker-compose -f docker-compose.integration.yml up --build --abort-on-container-exit
	docker-compose -f docker-compose.integration.yml down --remove-orphans

# Generate test coverage
coverage:
	@echo "📊 Generating test coverage..."
	cargo install cargo-tarpaulin --locked
	cargo tarpaulin --out Html --output-dir coverage

# Install the agent locally
install: build
	@echo "📦 Installing Sentinel Agent..."
	sudo ./install.sh --binary target/release/sentinel-agent

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	docker-compose -f docker-compose.integration.yml down --remove-orphans --volumes 2>/dev/null || true
	docker rmi sentinel-mock-api sentinel-agent 2>/dev/null || true

# Create a new release
release:
	@if [ -z "$(VERSION)" ]; then \
		echo "❌ VERSION is required. Usage: make release VERSION=v1.0.0"; \
		exit 1; \
	fi
	@echo "🚀 Creating release $(VERSION)..."
	@echo "📋 Running pre-release checks..."
	@$(MAKE) test
	@echo "✅ All tests passed!"
	@echo "🏷️  Creating and pushing tag..."
	git tag -a $(VERSION) -m "Release $(VERSION)"
	git push origin $(VERSION)
	@echo "🎉 Release $(VERSION) created! GitHub Actions will build and publish the release."
	@echo "📍 Track progress: https://github.com/operion/sentinel-agent/actions"

# Development workflow
dev: clean build test
	@echo "✅ Development workflow completed!"