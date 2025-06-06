use std::time::Duration;
use reqwest::Client;
use serde_json::Value;
use testcontainers::{clients::Cli, core::WaitFor, GenericImage};
use tokio::time::sleep;

#[tokio::test]
async fn test_sentinel_agent_integration() {
    let docker = Cli::default();
    
    // Start mock API server
    let mock_api = docker.run(
        GenericImage::new("sentinel-mock-api", "latest")
            .with_exposed_port(8080)
            .with_wait_for(WaitFor::message_on_stdout("Starting Mock Operion API Server"))
    );
    
    let api_port = mock_api.get_host_port_ipv4(8080);
    let api_url = format!("http://localhost:{}", api_port);
    
    // Wait for API to be ready
    let client = Client::new();
    let mut retries = 30;
    while retries > 0 {
        if let Ok(response) = client.get(&format!("{}/health", api_url)).send().await {
            if response.status().is_success() {
                break;
            }
        }
        sleep(Duration::from_millis(500)).await;
        retries -= 1;
    }
    assert!(retries > 0, "Mock API server failed to start");
    
    // Start Sentinel agent
    let agent_image = GenericImage::new("sentinel-agent", "latest")
        .with_env_var("API_ENDPOINT", &api_url)
        .with_wait_for(WaitFor::message_on_stdout("Starting Operion Sentinel Agent"));
    
    let _agent = docker.run(agent_image);
    
    // Wait for agent to send metrics
    sleep(Duration::from_secs(10)).await;
    
    // Verify metrics were received
    let stats_response = client
        .get(&format!("{}/stats", api_url))
        .send()
        .await
        .expect("Failed to get stats")
        .json::<Value>()
        .await
        .expect("Failed to parse stats JSON");
    
    let total_batches = stats_response["metrics_stats"]["total_batches_received"]
        .as_u64()
        .expect("Missing total_batches_received");
    
    let total_metrics = stats_response["metrics_stats"]["total_metrics_received"]
        .as_u64()
        .expect("Missing total_metrics_received");
    
    // Assert that metrics were received
    assert!(total_batches > 0, "No metric batches received");
    assert!(total_metrics > 0, "No individual metrics received");
    
    // Verify latest metrics structure
    let latest_response = client
        .get(&format!("{}/metrics/latest", api_url))
        .send()
        .await
        .expect("Failed to get latest metrics");
    
    assert!(latest_response.status().is_success(), "Failed to retrieve latest metrics");
    
    let latest_metrics = latest_response
        .json::<Value>()
        .await
        .expect("Failed to parse latest metrics JSON");
    
    // Validate metrics structure
    let batch = &latest_metrics["batch"];
    assert!(batch["agent_id"].is_string(), "Missing agent_id");
    assert!(batch["hostname"].is_string(), "Missing hostname");
    assert!(batch["timestamp"].is_number(), "Missing timestamp");
    assert!(batch["metrics"].is_array(), "Missing metrics array");
    
    // Validate individual metrics
    let metrics = batch["metrics"].as_array().expect("Metrics should be an array");
    assert!(!metrics.is_empty(), "Metrics array should not be empty");
    
    let first_metric = &metrics[0];
    assert!(first_metric["device"].is_string(), "Missing device");
    assert!(first_metric["mount_point"].is_string(), "Missing mount_point");
    assert!(first_metric["total_space_bytes"].is_number(), "Missing total_space_bytes");
    assert!(first_metric["used_space_bytes"].is_number(), "Missing used_space_bytes");
    assert!(first_metric["available_space_bytes"].is_number(), "Missing available_space_bytes");
    assert!(first_metric["usage_percentage"].is_number(), "Missing usage_percentage");
    
    println!("✅ Integration test passed!");
    println!("📊 Total batches received: {}", total_batches);
    println!("📈 Total metrics received: {}", total_metrics);
}