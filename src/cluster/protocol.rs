//! Cluster communication protocol
//!
//! Simple JSON-over-TCP protocol for cluster communication.
//! Messages are length-prefixed (4 bytes big-endian) followed by JSON payload.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Messages exchanged between cluster nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClusterMessage {
    /// Heartbeat with node status
    Heartbeat {
        node_name: String,
        mqtt_addr: String,
        assigned_groups: HashSet<String>,
        connection_count: u64,
    },

    /// Request to join the cluster
    JoinRequest {
        node_name: String,
        mqtt_addr: String,
        cluster_addr: String,
        assigned_groups: HashSet<String>,
    },

    /// Response to join request with cluster state
    JoinResponse {
        accepted: bool,
        nodes: Vec<NodeSnapshot>,
        group_assignments: Vec<GroupAssignment>,
    },

    /// Announce group assignment change
    GroupAssigned { group: String, node_name: String },

    /// Announce group unassignment (node going down)
    GroupUnassigned {
        group: String,
        previous_node: String,
    },

    /// Sync a retained message
    RetainedSync {
        topic: String,
        payload: Vec<u8>,
        source_node: String,
    },

    /// Request all retained messages for a topic pattern
    RetainedRequest { topic_pattern: String },
}

/// Snapshot of a node for join responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSnapshot {
    pub name: String,
    pub mqtt_addr: String,
    pub cluster_addr: String,
    pub assigned_groups: Vec<String>,
}

/// Group to node assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAssignment {
    pub group: String,
    pub node_name: String,
}

impl ClusterMessage {
    /// Serialize message to bytes (length-prefixed JSON)
    pub fn to_bytes(&self) -> Vec<u8> {
        let json = serde_json::to_vec(self).expect("Failed to serialize cluster message");
        let len = json.len() as u32;
        let mut bytes = len.to_be_bytes().to_vec();
        bytes.extend(json);
        bytes
    }

    /// Deserialize message from bytes (assumes length prefix already consumed)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_serialization() {
        let msg = ClusterMessage::Heartbeat {
            node_name: "node-1".to_string(),
            mqtt_addr: "192.168.1.1:1883".to_string(),
            assigned_groups: ["web-servers".to_string()].into_iter().collect(),
            connection_count: 100,
        };

        let bytes = msg.to_bytes();
        assert!(bytes.len() > 4); // At least length prefix

        // Skip length prefix
        let json_bytes = &bytes[4..];
        let decoded = ClusterMessage::from_bytes(json_bytes).unwrap();

        if let ClusterMessage::Heartbeat {
            node_name,
            connection_count,
            ..
        } = decoded
        {
            assert_eq!(node_name, "node-1");
            assert_eq!(connection_count, 100);
        } else {
            panic!("Wrong message type");
        }
    }
}
