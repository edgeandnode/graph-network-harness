//! Graph Protocol specific service types
//!
//! This module defines service types for Graph Protocol components that can be
//! managed by the harness daemon, implementing domain-specific functionality.

use async_trait::async_trait;
use harness_core::prelude::*;
use harness_core::ServiceConfig;
use service_orchestration::{ServiceTarget, HealthCheck};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Anvil blockchain service for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnvilBlockchain {
    /// Chain ID for the blockchain
    pub chain_id: u64,
    /// Port to run on (default: 8545)
    pub port: u16,
    /// Block time in seconds (None for instant mining)
    pub block_time: Option<u64>,
    /// Fork from another chain URL
    pub fork_url: Option<String>,
    /// Enable auto-mining
    pub auto_mine: bool,
}

impl Default for AnvilBlockchain {
    fn default() -> Self {
        Self {
            chain_id: 31337,
            port: 8545,
            block_time: None,
            fork_url: None,
            auto_mine: true,
        }
    }
}

#[async_trait]
impl ServiceType for AnvilBlockchain {
    fn type_name() -> &'static str {
        "anvil-blockchain"
    }
    
    fn to_service_config(&self, name: String) -> Result<ServiceConfig> {
        let mut args = vec![
            "--chain-id".to_string(),
            self.chain_id.to_string(),
            "--port".to_string(),
            self.port.to_string(),
            "--host".to_string(),
            "0.0.0.0".to_string(),
        ];
        
        if let Some(block_time) = self.block_time {
            args.extend(["--block-time".to_string(), block_time.to_string()]);
        }
        
        if let Some(fork_url) = &self.fork_url {
            args.extend(["--fork-url".to_string(), fork_url.clone()]);
        }
        
        if !self.auto_mine {
            args.push("--no-mining".to_string());
        }
        
        Ok(ServiceConfig {
            name,
            target: ServiceTarget::Process {
                binary: "anvil".to_string(),
                args,
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: Some(HealthCheck {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    r#"{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}"#.to_string(),
                    format!("http://localhost:{}", self.port),
                ],
                interval: 10,
                retries: 3,
                timeout: 5,
            }),
        })
    }
    
    fn validate(&self) -> Result<()> {
        if self.port == 0 {
            return Err(Error::service_type("Port cannot be 0"));
        }
        if self.chain_id == 0 {
            return Err(Error::service_type("Chain ID cannot be 0"));
        }
        Ok(())
    }
}

/// IPFS node service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsNode {
    /// IPFS API port (default: 5001)
    pub api_port: u16,
    /// IPFS gateway port (default: 8080)
    pub gateway_port: u16,
    /// Swarm port (default: 4001)
    pub swarm_port: u16,
    /// Enable experimental features
    pub experimental: bool,
}

impl Default for IpfsNode {
    fn default() -> Self {
        Self {
            api_port: 5001,
            gateway_port: 8080,
            swarm_port: 4001,
            experimental: true,
        }
    }
}

#[async_trait]
impl ServiceType for IpfsNode {
    fn type_name() -> &'static str {
        "ipfs-node"
    }
    
    fn to_service_config(&self, name: String) -> Result<ServiceConfig> {
        let mut env = HashMap::new();
        env.insert("IPFS_PATH".to_string(), "/data/ipfs".to_string());
        
        if self.experimental {
            env.insert("IPFS_ENABLE_EXPERIMENTAL".to_string(), "true".to_string());
        }
        
        Ok(ServiceConfig {
            name,
            target: ServiceTarget::Docker {
                image: "ipfs/go-ipfs:latest".to_string(),
                env,
                ports: vec![self.api_port, self.gateway_port, self.swarm_port],
                volumes: vec![
                    "/data/ipfs:/data/ipfs".to_string(),
                    "/data/ipfs/staging:/export".to_string(),
                ],
            },
            dependencies: vec![],
            health_check: Some(HealthCheck {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    format!("http://localhost:{}/api/v0/version", self.api_port),
                ],
                interval: 10,
                retries: 3,
                timeout: 5,
            }),
        })
    }
}

/// PostgreSQL database service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresDb {
    /// Database port (default: 5432)
    pub port: u16,
    /// Database name
    pub database: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
    /// Enable connection pooling
    pub pooling: bool,
}

impl Default for PostgresDb {
    fn default() -> Self {
        Self {
            port: 5432,
            database: "graph".to_string(),
            username: "postgres".to_string(),
            password: "password".to_string(),
            pooling: true,
        }
    }
}

#[async_trait]
impl ServiceType for PostgresDb {
    fn type_name() -> &'static str {
        "postgres-db"
    }
    
    fn to_service_config(&self, name: String) -> Result<ServiceConfig> {
        let mut env = HashMap::new();
        env.insert("POSTGRES_DB".to_string(), self.database.clone());
        env.insert("POSTGRES_USER".to_string(), self.username.clone());
        env.insert("POSTGRES_PASSWORD".to_string(), self.password.clone());
        
        Ok(ServiceConfig {
            name,
            target: ServiceTarget::Docker {
                image: "postgres:14".to_string(),
                env,
                ports: vec![self.port],
                volumes: vec!["/var/lib/postgresql/data:/var/lib/postgresql/data".to_string()],
            },
            dependencies: vec![],
            health_check: Some(HealthCheck {
                command: "pg_isready".to_string(),
                args: vec![
                    "-h".to_string(),
                    "localhost".to_string(),
                    "-p".to_string(),
                    self.port.to_string(),
                    "-U".to_string(),
                    self.username.clone(),
                ],
                interval: 10,
                retries: 3,
                timeout: 5,
            }),
        })
    }
}

/// Graph Node service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// HTTP port for GraphQL API (default: 8000)
    pub http_port: u16,
    /// WebSocket port (default: 8001)
    pub ws_port: u16,
    /// JSON-RPC port (default: 8020)
    pub json_rpc_port: u16,
    /// Index node port (default: 8030)
    pub index_port: u16,
    /// Metrics port (default: 8040)
    pub metrics_port: u16,
    /// Ethereum RPC URL
    pub ethereum_rpc: String,
    /// IPFS API URL
    pub ipfs_api: String,
    /// PostgreSQL connection string
    pub postgres_url: String,
    /// Node ID
    pub node_id: String,
}

impl Default for GraphNode {
    fn default() -> Self {
        Self {
            http_port: 8000,
            ws_port: 8001,
            json_rpc_port: 8020,
            index_port: 8030,
            metrics_port: 8040,
            ethereum_rpc: "http://localhost:8545".to_string(),
            ipfs_api: "http://localhost:5001".to_string(),
            postgres_url: "postgresql://postgres:password@localhost:5432/graph".to_string(),
            node_id: "default".to_string(),
        }
    }
}

#[async_trait]
impl ServiceType for GraphNode {
    fn type_name() -> &'static str {
        "graph-node"
    }
    
    fn to_service_config(&self, name: String) -> Result<ServiceConfig> {
        let mut env = HashMap::new();
        env.insert("postgres_host".to_string(), "localhost".to_string());
        env.insert("postgres_port".to_string(), "5432".to_string());
        env.insert("postgres_user".to_string(), "postgres".to_string());
        env.insert("postgres_pass".to_string(), "password".to_string());
        env.insert("postgres_db".to_string(), "graph".to_string());
        env.insert("ipfs".to_string(), self.ipfs_api.clone());
        env.insert("ethereum".to_string(), format!("mainnet:{}", self.ethereum_rpc));
        env.insert("GRAPH_NODE_ID".to_string(), self.node_id.clone());
        env.insert("GRAPH_LOG".to_string(), "info".to_string());
        
        Ok(ServiceConfig {
            name,
            target: ServiceTarget::Docker {
                image: "graphprotocol/graph-node:latest".to_string(),
                env,
                ports: vec![
                    self.http_port,
                    self.ws_port,
                    self.json_rpc_port,
                    self.index_port,
                    self.metrics_port,
                ],
                volumes: vec![],
            },
            dependencies: vec!["postgres".to_string(), "ipfs".to_string(), "anvil".to_string()],
            health_check: Some(HealthCheck {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    format!("http://localhost:{}/", self.http_port),
                ],
                interval: 15,
                retries: 5,
                timeout: 10,
            }),
        })
    }
    
    fn validate(&self) -> Result<()> {
        if self.ethereum_rpc.is_empty() {
            return Err(Error::service_type("Ethereum RPC URL cannot be empty"));
        }
        if self.ipfs_api.is_empty() {
            return Err(Error::service_type("IPFS API URL cannot be empty"));
        }
        if self.postgres_url.is_empty() {
            return Err(Error::service_type("PostgreSQL URL cannot be empty"));
        }
        Ok(())
    }
}