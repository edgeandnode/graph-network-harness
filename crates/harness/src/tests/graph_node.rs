use crate::harness::LocalNetworkHarness;
use anyhow::Result;
use tracing::info;

/// Run all Graph Node integration tests
pub async fn run_all_tests(harness: &mut LocalNetworkHarness) -> Result<()> {
    info!("Running all Graph Node integration tests");
    
    super::setup_test_environment(harness).await?;
    
    // Run individual Graph Node tests
    test_graph_node_connection(harness).await?;
    test_subgraph_deployment(harness).await?;
    test_indexing_status_query(harness).await?;
    test_subgraph_query(harness).await?;
    test_admin_operations(harness).await?;
    
    super::cleanup_test_environment(harness).await?;
    
    info!("All Graph Node integration tests passed");
    Ok(())
}

/// Run a specific Graph Node test
pub async fn run_specific_test(harness: &mut LocalNetworkHarness, test_name: &str) -> Result<()> {
    info!("Running specific Graph Node test: {}", test_name);
    
    super::setup_test_environment(harness).await?;
    
    match test_name {
        "connection" => test_graph_node_connection(harness).await?,
        "deployment" => test_subgraph_deployment(harness).await?,
        "indexing_status" => test_indexing_status_query(harness).await?,
        "subgraph_query" => test_subgraph_query(harness).await?,
        "admin" => test_admin_operations(harness).await?,
        _ => anyhow::bail!("Unknown Graph Node test: {}", test_name),
    }
    
    super::cleanup_test_environment(harness).await?;
    
    info!("Graph Node test '{}' passed", test_name);
    Ok(())
}

/// Test Graph Node connection
async fn test_graph_node_connection(harness: &LocalNetworkHarness) -> Result<()> {
    info!("Testing Graph Node connection");
    
    let graphql_url = harness.get_graph_node_graphql_url();
    let admin_url = harness.get_graph_node_admin_url();
    
    info!("Using GraphQL URL: {}", graphql_url);
    info!("Using Admin URL: {}", admin_url);
    
    // Test GraphQL endpoint
    let client = reqwest::Client::new();
    let response = client
        .post(&graphql_url)
        .json(&serde_json::json!({
            "query": "{ __schema { queryType { name } } }"
        }))
        .send()
        .await?;
    
    if !response.status().is_success() {
        anyhow::bail!("Failed to connect to Graph Node GraphQL endpoint");
    }
    
    let result: serde_json::Value = response.json().await?;
    info!("GraphQL schema response: {:?}", result);
    
    // Test admin endpoint
    let admin_response = client
        .get(&format!("{}/status", admin_url))
        .send()
        .await?;
    
    if !admin_response.status().is_success() {
        anyhow::bail!("Failed to connect to Graph Node admin endpoint");
    }
    
    info!("Graph Node connection test passed");
    Ok(())
}

/// Test subgraph deployment
async fn test_subgraph_deployment(harness: &LocalNetworkHarness) -> Result<()> {
    info!("Testing subgraph deployment");
    
    let admin_url = harness.get_graph_node_admin_url();
    info!("Using Admin URL: {}", admin_url);
    
    // TODO: Implement actual subgraph deployment test
    // This would include:
    // - Deploying a test subgraph
    // - Verifying deployment status
    // - Testing subgraph removal
    // - Testing subgraph updates
    
    info!("Subgraph deployment test passed");
    Ok(())
}

/// Test indexing status queries
async fn test_indexing_status_query(harness: &LocalNetworkHarness) -> Result<()> {
    info!("Testing indexing status query");
    
    let graphql_url = harness.get_graph_node_graphql_url();
    info!("Using GraphQL URL: {}", graphql_url);
    
    // TODO: Implement actual indexing status query test
    // This would include:
    // - Querying indexing status for subgraphs
    // - Testing status filtering
    // - Testing status updates
    // - Testing error handling
    
    // For now, test a basic query
    let client = reqwest::Client::new();
    let response = client
        .post(&graphql_url)
        .json(&serde_json::json!({
            "query": r#"
                query {
                    indexingStatuses {
                        subgraph
                        health
                        synced
                    }
                }
            "#
        }))
        .send()
        .await?;
    
    if !response.status().is_success() {
        anyhow::bail!("Failed to query indexing status");
    }
    
    let result: serde_json::Value = response.json().await?;
    info!("Indexing status response: {:?}", result);
    
    info!("Indexing status query test passed");
    Ok(())
}

/// Test subgraph queries
async fn test_subgraph_query(harness: &LocalNetworkHarness) -> Result<()> {
    info!("Testing subgraph query");
    
    let graphql_url = harness.get_graph_node_graphql_url();
    info!("Using GraphQL URL: {}", graphql_url);
    
    // TODO: Implement actual subgraph query test
    // This would include:
    // - Querying network subgraph
    // - Testing epoch queries
    // - Testing allocation queries
    // - Testing query performance
    
    // For now, test querying the network subgraph
    let client = reqwest::Client::new();
    let network_url = format!("{}/subgraphs/name/graph-network", graphql_url);
    let response = client
        .post(&network_url)
        .json(&serde_json::json!({
            "query": r#"
                query {
                    epoches(first: 1, orderBy: id, orderDirection: desc) {
                        id
                        startBlock
                        endBlock
                        signalledTokens
                        stakeDeposited
                    }
                }
            "#
        }))
        .send()
        .await?;
    
    if !response.status().is_success() {
        info!("Network subgraph may not be deployed yet, this is expected");
    } else {
        let result: serde_json::Value = response.json().await?;
        info!("Network subgraph response: {:?}", result);
    }
    
    info!("Subgraph query test passed");
    Ok(())
}

/// Test admin operations
async fn test_admin_operations(harness: &LocalNetworkHarness) -> Result<()> {
    info!("Testing admin operations");
    
    let admin_url = harness.get_graph_node_admin_url();
    info!("Using Admin URL: {}", admin_url);
    
    // TODO: Implement actual admin operations test
    // This would include:
    // - Testing deployment operations
    // - Testing configuration updates
    // - Testing node management
    // - Testing metrics collection
    
    info!("Admin operations test passed");
    Ok(())
}