//! Cluster module for HA support
//!
//! Implements:
//! - Node discovery (static peers, DNS, etcd)
//! - Group-based sharding (agent_group -> node mapping)
//! - Health monitoring and automatic failover
//! - Retained message synchronization

mod coordinator;
mod node;
mod protocol;
mod retained;

pub use coordinator::ClusterCoordinator;
pub use node::{NodeInfo, NodeState};
pub use protocol::ClusterMessage;
pub use retained::{RetainedMessage, RetainedStore};
