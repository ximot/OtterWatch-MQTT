//! Configuration schema for otterwatch-mqtt broker

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Node identification and storage
    pub node: NodeConfig,

    /// MQTT broker settings
    pub mqtt: MqttConfig,

    /// Authentication settings
    pub auth: AuthConfig,

    /// Cluster/HA settings (optional)
    #[serde(default)]
    pub cluster: ClusterConfig,

    /// Bridge configuration for connecting to other brokers
    #[serde(default)]
    pub bridge: Vec<BridgeConfig>,

    /// Prometheus metrics settings
    #[serde(default)]
    pub metrics: MetricsConfig,

    /// Management API settings
    #[serde(default)]
    pub api: ApiConfig,
}

/// Node identification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node name in the cluster
    #[serde(default = "default_node_name")]
    pub name: String,

    /// Directory for persistent data (retained messages, state)
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

fn default_node_name() -> String {
    format!(
        "otterwatch-mqtt-{}",
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    )
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}

/// MQTT broker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    /// Listen address for MQTT connections
    #[serde(default = "default_mqtt_listen")]
    pub listen_addr: String,

    /// Public/advertise address for clients to connect to
    /// If not set, listen_addr is used (which may not work if binding to 0.0.0.0)
    #[serde(default)]
    pub public_addr: Option<String>,

    /// Maximum number of concurrent connections
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Router buffer size for message queuing
    #[serde(default = "default_router_buffer")]
    pub router_buffer_size: usize,

    /// Client keepalive timeout in seconds
    #[serde(default = "default_keepalive")]
    pub keepalive_secs: u16,

    /// Maximum packet size in bytes
    #[serde(default = "default_max_packet_size")]
    pub max_packet_size: usize,

    /// Maximum QoS level (0, 1, or 2)
    #[serde(default = "default_max_qos")]
    pub max_qos: u8,
}

fn default_mqtt_listen() -> String {
    "0.0.0.0:1883".to_string()
}

fn default_max_connections() -> usize {
    10000
}

fn default_router_buffer() -> usize {
    50000
}

fn default_keepalive() -> u16 {
    30
}

fn default_max_packet_size() -> usize {
    256 * 1024 // 256KB
}

fn default_max_qos() -> u8 {
    1
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_mqtt_listen(),
            public_addr: None,
            max_connections: default_max_connections(),
            router_buffer_size: default_router_buffer(),
            keepalive_secs: default_keepalive(),
            max_packet_size: default_max_packet_size(),
            max_qos: default_max_qos(),
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// List of valid API keys (stored as SHA256 hashes internally)
    #[serde(default)]
    pub api_keys: Vec<String>,

    /// Whether authentication is required
    #[serde(default = "default_require_auth")]
    pub require_auth: bool,

    /// Where to look for API key in MQTT CONNECT packet
    #[serde(default)]
    pub api_key_source: ApiKeySource,
}

fn default_require_auth() -> bool {
    true
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            api_keys: Vec::new(),
            require_auth: true,
            api_key_source: ApiKeySource::default(),
        }
    }
}

/// Where to extract API key from MQTT CONNECT
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeySource {
    /// Use MQTT password field (default, compatible with OtterWatch agents)
    #[default]
    Password,
    /// Use MQTT username field
    Username,
}

/// Cluster/HA configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Whether clustering is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Bind address for cluster communication
    #[serde(default = "default_cluster_bind")]
    pub bind_addr: String,

    /// Groups assigned to this node (empty = accept all)
    #[serde(default)]
    pub assigned_groups: Vec<String>,

    /// Node discovery configuration
    #[serde(default)]
    pub discovery: DiscoveryConfig,
}

fn default_cluster_bind() -> String {
    "0.0.0.0:7000".to_string()
}

/// Cluster node discovery configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Static peer addresses
    #[serde(default)]
    pub static_peers: Vec<String>,

    /// DNS name for SRV record discovery
    pub dns_name: Option<String>,

    /// etcd endpoints for discovery (requires cluster-etcd feature)
    #[serde(default)]
    pub etcd_endpoints: Vec<String>,
}

/// Prometheus metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Whether metrics are enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Listen address for metrics HTTP endpoint
    #[serde(default = "default_metrics_listen")]
    pub listen_addr: String,
}

fn default_true() -> bool {
    true
}

fn default_metrics_listen() -> String {
    "0.0.0.0:9090".to_string()
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_addr: default_metrics_listen(),
        }
    }
}

/// Management API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Listen address for API HTTP endpoint
    #[serde(default = "default_api_listen")]
    pub listen_addr: String,

    /// Whether the web dashboard is enabled
    #[serde(default = "default_true")]
    pub dashboard_enabled: bool,

    /// CORS allowed origins
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
}

fn default_api_listen() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_cors_origins() -> Vec<String> {
    vec!["*".to_string()]
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_api_listen(),
            dashboard_enabled: true,
            cors_origins: default_cors_origins(),
        }
    }
}

/// Bridge configuration for connecting to another MQTT broker
/// This allows message forwarding between broker nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// Bridge name (for identification in logs)
    pub name: String,

    /// Remote broker address (e.g., "192.168.1.100:1884")
    pub addr: String,

    /// Topic filter to subscribe and forward (e.g., "otterwatch/#")
    #[serde(default = "default_bridge_topic")]
    pub topic: String,

    /// QoS level for bridge subscription (0, 1, or 2)
    #[serde(default = "default_bridge_qos")]
    pub qos: u8,

    /// Reconnection delay in seconds
    #[serde(default = "default_bridge_reconnect_delay")]
    pub reconnection_delay_secs: u64,

    /// Ping/keepalive interval in seconds
    #[serde(default = "default_bridge_ping_delay")]
    pub ping_delay_secs: u64,
}

fn default_bridge_topic() -> String {
    "otterwatch/#".to_string()
}

fn default_bridge_qos() -> u8 {
    1
}

fn default_bridge_reconnect_delay() -> u64 {
    5
}

fn default_bridge_ping_delay() -> u64 {
    30
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config: Config = toml::from_str(
            r#"
            [node]
            name = "test-node"

            [mqtt]

            [auth]
            api_keys = ["key1", "key2"]
            "#,
        )
        .unwrap();

        assert_eq!(config.node.name, "test-node");
        assert_eq!(config.mqtt.max_connections, 10000);
        assert_eq!(config.auth.api_keys.len(), 2);
        assert!(config.auth.require_auth);
    }
}
