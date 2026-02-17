//! Node information and state tracking

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Instant;

/// State of a cluster node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
    /// Node is healthy and accepting connections
    Healthy,
    /// Node hasn't responded to recent health checks
    Suspect,
    /// Node is confirmed down
    Down,
    /// Node is joining the cluster
    Joining,
}

impl Default for NodeState {
    fn default() -> Self {
        Self::Joining
    }
}

/// Information about a cluster node
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// Unique node name
    pub name: String,
    /// Address for cluster communication
    pub cluster_addr: SocketAddr,
    /// Address for MQTT connections (what clients connect to)
    pub mqtt_addr: SocketAddr,
    /// Current state of the node
    pub state: NodeState,
    /// Groups assigned to this node
    pub assigned_groups: HashSet<String>,
    /// Last time we received a heartbeat from this node
    pub last_heartbeat: Instant,
    /// Number of active connections on this node
    pub connection_count: u64,
    /// Whether this is the local node
    pub is_local: bool,
}

impl NodeInfo {
    /// Create info for the local node
    pub fn local(
        name: String,
        cluster_addr: SocketAddr,
        mqtt_addr: SocketAddr,
        assigned_groups: Vec<String>,
    ) -> Self {
        Self {
            name,
            cluster_addr,
            mqtt_addr,
            state: NodeState::Healthy,
            assigned_groups: assigned_groups.into_iter().collect(),
            last_heartbeat: Instant::now(),
            connection_count: 0,
            is_local: true,
        }
    }

    /// Create info for a remote peer (initially unknown state)
    pub fn remote(name: String, cluster_addr: SocketAddr, mqtt_addr: SocketAddr) -> Self {
        Self {
            name,
            cluster_addr,
            mqtt_addr,
            state: NodeState::Joining,
            assigned_groups: HashSet::new(),
            last_heartbeat: Instant::now(),
            connection_count: 0,
            is_local: false,
        }
    }

    /// Create info for a remote peer with known assigned groups
    pub fn remote_with_groups(
        name: String,
        cluster_addr: SocketAddr,
        mqtt_addr: SocketAddr,
        assigned_groups: HashSet<String>,
    ) -> Self {
        Self {
            name,
            cluster_addr,
            mqtt_addr,
            state: NodeState::Healthy,
            assigned_groups,
            last_heartbeat: Instant::now(),
            connection_count: 0,
            is_local: false,
        }
    }

    /// Update from a heartbeat message
    pub fn update_heartbeat(&mut self, groups: HashSet<String>, connections: u64) {
        self.last_heartbeat = Instant::now();
        self.assigned_groups = groups;
        self.connection_count = connections;
        self.state = NodeState::Healthy;
    }

    /// Check if node should be marked suspect (no heartbeat for 10s)
    pub fn check_health(&mut self, suspect_after_secs: u64, down_after_secs: u64) {
        let elapsed = self.last_heartbeat.elapsed().as_secs();

        if elapsed > down_after_secs {
            self.state = NodeState::Down;
        } else if elapsed > suspect_after_secs {
            self.state = NodeState::Suspect;
        }
    }

    /// Check if this node handles the given group
    pub fn handles_group(&self, group: &str) -> bool {
        // Empty assigned_groups means "handle all groups"
        self.assigned_groups.is_empty() || self.assigned_groups.contains(group)
    }
}

/// Serializable node info for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfoResponse {
    pub name: String,
    pub cluster_addr: String,
    pub mqtt_addr: String,
    pub state: NodeState,
    pub assigned_groups: Vec<String>,
    pub connection_count: u64,
    pub is_local: bool,
}

impl From<&NodeInfo> for NodeInfoResponse {
    fn from(node: &NodeInfo) -> Self {
        Self {
            name: node.name.clone(),
            cluster_addr: node.cluster_addr.to_string(),
            mqtt_addr: node.mqtt_addr.to_string(),
            state: node.state,
            assigned_groups: node.assigned_groups.iter().cloned().collect(),
            connection_count: node.connection_count,
            is_local: node.is_local,
        }
    }
}
