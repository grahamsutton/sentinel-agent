#!/usr/bin/env python3
"""
Mock Operion API Server for integration testing
Lightweight Flask-based server for CI/CD pipelines
"""

import json
import time
from datetime import datetime
from flask import Flask, request, jsonify
from flask_cors import CORS
import logging
import threading
import os

app = Flask(__name__)
CORS(app)

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Global state for tracking metrics
metrics_store = []
server_stats = {
    'start_time': time.time(),
    'total_batches': 0,
    'total_metrics': 0,
    'last_received': None
}

@app.route('/health', methods=['GET'])
def health_check():
    """Health check endpoint for container readiness"""
    return jsonify({
        'status': 'healthy',
        'timestamp': datetime.now().isoformat(),
        'uptime_seconds': time.time() - server_stats['start_time']
    })

@app.route('/api/v1/servers', methods=['POST'])
def register_server():
    """Register a new server/agent"""
    try:
        registration = request.get_json()

        if not registration:
            return jsonify({'error': 'No JSON payload'}), 400

        # Validate required fields
        required_fields = ['agent_id', 'hostname', 'agent_version', 'platform', 'arch']
        for field in required_fields:
            if field not in registration:
                return jsonify({'error': f'Missing required field: {field}'}), 400

        logger.info(f"Server registration: {registration['agent_id']} ({registration['hostname']})")

        return jsonify({
            'server_id': f"srv_{registration['agent_id']}",
            'status': 'registered',
            'message': 'Server registered successfully'
        }), 201

    except Exception as e:
        logger.error(f"Error processing server registration: {e}")
        return jsonify({'error': str(e)}), 500

@app.route('/api/v1/metrics', methods=['POST'])
def receive_metrics():
    """Receive metrics from Sentinel agents"""
    try:
        metrics_batch = request.get_json()
        
        if not metrics_batch:
            return jsonify({'error': 'No JSON payload'}), 400
        
        # Validate required fields
        required_fields = ['server_id', 'hostname', 'metrics']
        for field in required_fields:
            if field not in metrics_batch:
                return jsonify({'error': f'Missing required field: {field}'}), 400
        
        # Update statistics
        server_id = metrics_batch['server_id']
        hostname = metrics_batch['hostname']
        metrics_count = len(metrics_batch['metrics'])
        
        server_stats['total_batches'] += 1
        server_stats['total_metrics'] += metrics_count
        server_stats['last_received'] = time.time()
        
        # Store the batch
        metrics_store.append({
            'timestamp': time.time(),
            'batch': metrics_batch
        })
        
        logger.info(f"Received {metrics_count} metrics from {server_id} ({hostname})")
        
        # Log individual metrics for debugging
        for metric in metrics_batch['metrics']:
            device = metric.get('device', 'unknown')
            mount_point = metric.get('mount_point', 'unknown')
            usage_pct = metric.get('usage_percentage', 0)
            logger.info(f"  {device} ({mount_point}): {usage_pct:.1f}% used")
        
        return jsonify({
            'status': 'success',
            'received_metrics': metrics_count,
            'timestamp': datetime.now().isoformat()
        })
        
    except Exception as e:
        logger.error(f"Error processing metrics: {e}")
        return jsonify({'error': str(e)}), 500

@app.route('/stats', methods=['GET'])
def get_stats():
    """Get server statistics for testing validation"""
    uptime = time.time() - server_stats['start_time']
    
    return jsonify({
        'server_info': {
            'status': 'running',
            'uptime_seconds': uptime,
            'start_time': datetime.fromtimestamp(server_stats['start_time']).isoformat()
        },
        'metrics_stats': {
            'total_batches_received': server_stats['total_batches'],
            'total_metrics_received': server_stats['total_metrics'],
            'last_metric_received': datetime.fromtimestamp(server_stats['last_received']).isoformat() if server_stats['last_received'] else None,
            'stored_batches': len(metrics_store)
        }
    })

@app.route('/metrics/latest', methods=['GET'])
def get_latest_metrics():
    """Get the most recent metrics batch for test validation"""
    if not metrics_store:
        return jsonify({'error': 'No metrics received yet'}), 404
    
    latest = metrics_store[-1]
    return jsonify({
        'received_at': datetime.fromtimestamp(latest['timestamp']).isoformat(),
        'batch': latest['batch']
    })

@app.route('/metrics/all', methods=['GET'])
def get_all_metrics():
    """Get all received metrics (for debugging)"""
    return jsonify({
        'total_batches': len(metrics_store),
        'metrics': [
            {
                'received_at': datetime.fromtimestamp(item['timestamp']).isoformat(),
                'batch': item['batch']
            }
            for item in metrics_store
        ]
    })

@app.route('/reset', methods=['POST'])
def reset_server():
    """Reset server state (useful for test isolation)"""
    global metrics_store
    metrics_store = []
    server_stats.update({
        'total_batches': 0,
        'total_metrics': 0,
        'last_received': None
    })
    
    logger.info("Server state reset")
    return jsonify({'status': 'reset', 'timestamp': datetime.now().isoformat()})

if __name__ == '__main__':
    print("🚀 Starting Mock Operion API Server...")
    print("📍 Health check: GET /health")
    print("📊 Metrics endpoint: POST /api/v1/metrics")
    print("📈 Statistics: GET /stats")
    print("🔄 Reset: POST /reset")
    
    # Run with threading for better performance in containers
    app.run(host='0.0.0.0', port=8080, debug=False, threaded=True)