#!/bin/bash
set -e

echo "🚀 Starting Sentinel Agent Integration Test"
echo "=========================================="

# Build Docker images
echo "🐳 Building Docker images..."
docker build -f tests/integration/Dockerfile.mock-api -t sentinel-mock-api .
docker build -f tests/integration/Dockerfile.agent-builder -t sentinel-agent .

# Clean up any existing containers
echo "🧹 Cleaning up existing containers..."
docker-compose -f docker-compose.integration.yml down --remove-orphans 2>/dev/null || true

# Start the integration test environment
echo "🏁 Starting integration test environment..."
docker-compose -f docker-compose.integration.yml up -d

# Wait for services to be healthy
echo "⏳ Waiting for services to be ready..."
timeout=60
while [ $timeout -gt 0 ]; do
    if docker-compose -f docker-compose.integration.yml ps | grep -q "healthy"; then
        echo "✅ Services are healthy!"
        break
    fi
    sleep 2
    timeout=$((timeout - 2))
done

if [ $timeout -le 0 ]; then
    echo "❌ Services failed to become healthy within timeout"
    docker-compose -f docker-compose.integration.yml logs
    docker-compose -f docker-compose.integration.yml down
    exit 1
fi

# Wait for metrics to be collected and sent
echo "📊 Waiting for metrics collection (15 seconds)..."
sleep 15

# Check if metrics were received
echo "🔍 Validating metrics collection..."
STATS=$(curl -s http://localhost:8080/stats 2>/dev/null)
if [ $? -ne 0 ]; then
    echo "❌ Failed to connect to API server"
    SUCCESS=false
else
    TOTAL_BATCHES=$(echo $STATS | jq -r '.metrics_stats.total_batches_received' 2>/dev/null || echo "0")
    TOTAL_METRICS=$(echo $STATS | jq -r '.metrics_stats.total_metrics_received' 2>/dev/null || echo "0")

    echo "📈 Integration Test Results:"
    echo "   Total batches received: $TOTAL_BATCHES"
    echo "   Total metrics received: $TOTAL_METRICS"

    # Validate results (expect at least 3 batches and 10 metrics in 15 seconds)
    if [ "$TOTAL_BATCHES" -ge 3 ] && [ "$TOTAL_METRICS" -ge 10 ]; then
        echo "✅ Integration test PASSED!"
        echo "🎉 Sentinel Agent successfully installed and sending metrics!"
        
        # Show latest metrics for verification
        echo ""
        echo "📊 Latest metrics sample:"
        curl -s http://localhost:8080/metrics/latest 2>/dev/null | jq '.batch.metrics[0]' 2>/dev/null || echo "No metrics to display"
        
        SUCCESS=true
    else
        echo "❌ Integration test FAILED!"
        echo "   Expected: ≥3 batches and ≥10 metrics"
        echo "   Received: $TOTAL_BATCHES batches and $TOTAL_METRICS metrics"
        echo ""
        echo "🔍 Container logs:"
        docker-compose -f docker-compose.integration.yml logs
        SUCCESS=false
    fi
fi

# Cleanup
echo ""
echo "🧹 Cleaning up..."
docker-compose -f docker-compose.integration.yml down --remove-orphans

if [ "$SUCCESS" = true ]; then
    echo "✅ Integration test completed successfully!"
    exit 0
else
    echo "❌ Integration test failed!"
    exit 1
fi