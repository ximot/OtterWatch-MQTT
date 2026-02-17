//! Cluster coordinator
//!
//! Manages cluster membership, group assignments, and inter-node communication.

use super::node::{NodeInfo, NodeInfoResponse, NodeState};
use super::protocol::{ClusterMessage, GroupAssignment, NodeSnapshot};
use super::retained::{RetainedMessage, RetainedStore};
use crate::config::ClusterConfig;
use dashmap::DashMap;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};

/// Cluster coordinator manages the cluster state
pub struct ClusterCoordinator {
    /// Configuration
    config: ClusterConfig,
    /// Local node info
    local_node: Arc<RwLock<NodeInfo>>,
    /// Known peer nodes
    peers: Arc<DashMap<String, NodeInfo>>,
    /// Group to node assignments
    group_assignments: Arc<RwLock<HashMap<String, String>>>,
    /// Retained message store
    retained_store: RetainedStore,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Whether cluster is enabled
    enabled: bool,
}

impl ClusterCoordinator {
    /// Create a disabled coordinator (single-node mode)
    pub fn disabled() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            config: ClusterConfig::default(),
            local_node: Arc::new(RwLock::new(NodeInfo::local(
                "standalone".to_string(),
                "127.0.0.1:7000".parse().unwrap(),
                "127.0.0.1:1883".parse().unwrap(),
                vec![],
            ))),
            peers: Arc::new(DashMap::new()),
            group_assignments: Arc::new(RwLock::new(HashMap::new())),
            retained_store: RetainedStore::new("standalone".to_string()),
            shutdown_tx,
            enabled: false,
        }
    }

    /// Create a new cluster coordinator
    pub fn new(config: ClusterConfig, node_name: String, mqtt_addr: SocketAddr) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        let cluster_addr: SocketAddr = config.bind_addr.parse().expect("Invalid cluster.bind_addr");

        let local_node = NodeInfo::local(
            node_name.clone(),
            cluster_addr,
            mqtt_addr,
            config.assigned_groups.clone(),
        );

        Self {
            config: config.clone(),
            local_node: Arc::new(RwLock::new(local_node)),
            peers: Arc::new(DashMap::new()),
            group_assignments: Arc::new(RwLock::new(HashMap::new())),
            retained_store: RetainedStore::new(node_name),
            shutdown_tx,
            enabled: config.enabled,
        }
    }

    /// Check if clustering is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get shutdown sender for graceful shutdown
    pub fn shutdown_sender(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Start the cluster coordinator
    pub async fn start(&self) -> anyhow::Result<()> {
        if !self.enabled {
            tracing::info!("Cluster mode disabled, running standalone");
            return Ok(());
        }

        let bind_addr: SocketAddr = self.config.bind_addr.parse()?;
        tracing::info!("Starting cluster coordinator on {}", bind_addr);

        // Start listening for peer connections
        let listener = TcpListener::bind(bind_addr).await?;
        let peers = self.peers.clone();
        let local_node = self.local_node.clone();
        let group_assignments = self.group_assignments.clone();
        let retained_store = self.retained_store.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Spawn listener task
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                tracing::debug!("Cluster connection from {}", addr);
                                let peers = peers.clone();
                                let local_node = local_node.clone();
                                let group_assignments = group_assignments.clone();
                                let retained_store = retained_store.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_peer_connection(
                                        stream,
                                        peers,
                                        local_node,
                                        group_assignments,
                                        retained_store,
                                    ).await {
                                        tracing::warn!("Peer connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Cluster coordinator shutting down");
                        break;
                    }
                }
            }
        });

        // Connect to static peers
        self.connect_to_static_peers().await;

        // Start heartbeat sender
        self.start_heartbeat_sender().await;

        // Start health checker
        self.start_health_checker().await;

        Ok(())
    }

    /// Connect to static peers from config
    async fn connect_to_static_peers(&self) {
        for peer_addr in &self.config.discovery.static_peers {
            let peer_addr = peer_addr.clone();
            let peers = self.peers.clone();
            let local_node = self.local_node.clone();

            tokio::spawn(async move {
                if let Err(e) = connect_to_peer(&peer_addr, peers, local_node).await {
                    tracing::warn!("Failed to connect to peer {}: {}", peer_addr, e);
                }
            });
        }
    }

    /// Start periodic heartbeat sender
    async fn start_heartbeat_sender(&self) {
        let peers = self.peers.clone();
        let local_node = self.local_node.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let node = local_node.read().await;
                        let heartbeat = ClusterMessage::Heartbeat {
                            node_name: node.name.clone(),
                            mqtt_addr: node.mqtt_addr.to_string(),
                            assigned_groups: node.assigned_groups.clone(),
                            connection_count: node.connection_count,
                        };
                        drop(node);

                        // Send heartbeat to all peers
                        for peer in peers.iter() {
                            if let Err(e) = send_message_to_peer(&peer.cluster_addr, &heartbeat).await {
                                tracing::trace!("Failed to send heartbeat to {}: {}", peer.name, e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });
    }

    /// Start periodic health checker
    async fn start_health_checker(&self) {
        let peers = self.peers.clone();
        let group_assignments = self.group_assignments.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check health of all peers
                        let mut down_nodes = Vec::new();

                        for mut peer in peers.iter_mut() {
                            peer.check_health(10, 30); // Suspect after 10s, down after 30s

                            if peer.state == NodeState::Down {
                                down_nodes.push(peer.name.clone());
                            }
                        }

                        // Handle failed nodes
                        for node_name in down_nodes {
                            tracing::warn!("Node {} is down, reassigning groups", node_name);
                            if let Some((_, _node)) = peers.remove(&node_name) {
                                // Remove group assignments for this node
                                let mut assignments = group_assignments.write().await;
                                assignments.retain(|_, assigned_node| assigned_node != &node_name);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });
    }

    /// Update local node connection count
    pub async fn update_connection_count(&self, count: u64) {
        let mut node = self.local_node.write().await;
        node.connection_count = count;
    }

    /// Find which node handles a specific group
    pub async fn find_node_for_group(&self, group: &str) -> Option<NodeInfoResponse> {
        // First check local node
        {
            let local = self.local_node.read().await;
            if local.handles_group(group) {
                return Some(NodeInfoResponse::from(&*local));
            }
        }

        // Check group assignments
        let assignments = self.group_assignments.read().await;
        if let Some(node_name) = assignments.get(group) {
            if let Some(peer) = self.peers.get(node_name) {
                return Some(NodeInfoResponse::from(&*peer));
            }
        }

        // Check peers that might handle all groups
        for peer in self.peers.iter() {
            if peer.handles_group(group) {
                return Some(NodeInfoResponse::from(&*peer));
            }
        }

        None
    }

    /// Get list of all nodes
    pub async fn get_nodes(&self) -> Vec<NodeInfoResponse> {
        let mut nodes = Vec::new();

        // Add local node
        {
            let local = self.local_node.read().await;
            nodes.push(NodeInfoResponse::from(&*local));
        }

        // Add peers
        for peer in self.peers.iter() {
            nodes.push(NodeInfoResponse::from(&*peer));
        }

        nodes
    }

    /// Get group assignments (combines static config + dynamic assignments)
    pub async fn get_group_assignments(&self) -> HashMap<String, String> {
        let mut assignments = HashMap::new();

        // Add local node's assigned groups
        {
            let local = self.local_node.read().await;
            for group in &local.assigned_groups {
                assignments.insert(group.clone(), local.name.clone());
            }
        }

        // Add peer nodes' assigned groups
        for peer in self.peers.iter() {
            for group in &peer.assigned_groups {
                assignments.insert(group.clone(), peer.name.clone());
            }
        }

        // Override with dynamic assignments (these take priority)
        let dynamic = self.group_assignments.read().await;
        for (group, node) in dynamic.iter() {
            assignments.insert(group.clone(), node.clone());
        }

        assignments
    }

    /// Get the retained message store
    pub fn retained_store(&self) -> &RetainedStore {
        &self.retained_store
    }

    /// Store a retained message and sync to peers
    pub async fn store_retained(&self, topic: String, payload: Vec<u8>) {
        self.retained_store.store(topic.clone(), payload.clone());

        // Sync to all peers
        let node_name = self.local_node.read().await.name.clone();
        let sync_msg = ClusterMessage::RetainedSync {
            topic,
            payload,
            source_node: node_name,
        };

        for peer in self.peers.iter() {
            if let Err(e) = send_message_to_peer(&peer.cluster_addr, &sync_msg).await {
                tracing::debug!("Failed to sync retained message to {}: {}", peer.name, e);
            }
        }
    }

    /// Request retained messages from peers for a topic pattern
    pub async fn request_retained(&self, topic_pattern: &str) {
        let request = ClusterMessage::RetainedRequest {
            topic_pattern: topic_pattern.to_string(),
        };

        for peer in self.peers.iter() {
            if let Err(e) = send_message_to_peer(&peer.cluster_addr, &request).await {
                tracing::debug!("Failed to request retained from {}: {}", peer.name, e);
            }
        }
    }

    /// Get all retained messages (for API)
    pub fn get_retained_messages(&self) -> Vec<RetainedMessage> {
        self.retained_store.get_all()
    }

    /// Get count of retained messages
    pub fn retained_count(&self) -> usize {
        self.retained_store.count()
    }

    /// Reassign a group to a different node
    /// Returns true if the assignment was changed
    pub async fn reassign_group(&self, group: String, target_node: String) -> bool {
        // Verify the target node exists
        let node_exists = {
            let local = self.local_node.read().await;
            if local.name == target_node {
                true
            } else {
                self.peers.contains_key(&target_node)
            }
        };

        if !node_exists {
            tracing::warn!(
                "Cannot reassign group {} to unknown node {}",
                group,
                target_node
            );
            return false;
        }

        // Update local assignment
        {
            let mut assignments = self.group_assignments.write().await;
            let old_node = assignments.get(&group).cloned();

            // Skip if already assigned to target
            if old_node.as_ref() == Some(&target_node) {
                return false;
            }

            assignments.insert(group.clone(), target_node.clone());
            tracing::info!(
                "Group {} reassigned: {:?} -> {}",
                group,
                old_node,
                target_node
            );
        }

        // Notify all peers about the change
        let assignment_msg = ClusterMessage::GroupAssigned {
            group: group.clone(),
            node_name: target_node.clone(),
        };

        for peer in self.peers.iter() {
            if let Err(e) = send_message_to_peer(&peer.cluster_addr, &assignment_msg).await {
                tracing::debug!(
                    "Failed to notify {} about group reassignment: {}",
                    peer.name,
                    e
                );
            }
        }

        true
    }

    /// Unassign a group (remove explicit assignment)
    pub async fn unassign_group(&self, group: String) -> bool {
        let previous_node = {
            let mut assignments = self.group_assignments.write().await;
            assignments.remove(&group)
        };

        if let Some(prev) = &previous_node {
            tracing::info!("Group {} unassigned from {}", group, prev);

            // Notify all peers
            let unassign_msg = ClusterMessage::GroupUnassigned {
                group: group.clone(),
                previous_node: prev.clone(),
            };

            for peer in self.peers.iter() {
                if let Err(e) = send_message_to_peer(&peer.cluster_addr, &unassign_msg).await {
                    tracing::debug!(
                        "Failed to notify {} about group unassignment: {}",
                        peer.name,
                        e
                    );
                }
            }

            true
        } else {
            false
        }
    }

    /// Get local node name
    pub async fn local_node_name(&self) -> String {
        self.local_node.read().await.name.clone()
    }

    /// Get list of available node names
    pub async fn available_nodes(&self) -> Vec<String> {
        let mut nodes = Vec::new();
        nodes.push(self.local_node.read().await.name.clone());
        for peer in self.peers.iter() {
            nodes.push(peer.name.clone());
        }
        nodes
    }
}

/// Handle an incoming peer connection
async fn handle_peer_connection(
    mut stream: TcpStream,
    peers: Arc<DashMap<String, NodeInfo>>,
    local_node: Arc<RwLock<NodeInfo>>,
    group_assignments: Arc<RwLock<HashMap<String, String>>>,
    retained_store: RetainedStore,
) -> anyhow::Result<()> {
    // Read message length
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    // Read message body
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let message = ClusterMessage::from_bytes(&body)?;

    match message {
        ClusterMessage::JoinRequest {
            node_name,
            mqtt_addr,
            cluster_addr,
            assigned_groups,
        } => {
            tracing::info!(
                "Node {} requesting to join cluster (groups: {:?})",
                node_name,
                assigned_groups
            );

            // Add to peers with their assigned groups
            let peer_cluster_addr: SocketAddr = cluster_addr.parse()?;
            let peer_mqtt_addr: SocketAddr = mqtt_addr.parse()?;
            let peer = NodeInfo::remote_with_groups(
                node_name.clone(),
                peer_cluster_addr,
                peer_mqtt_addr,
                assigned_groups,
            );
            peers.insert(node_name.clone(), peer);

            // Build response with current cluster state
            let nodes: Vec<NodeSnapshot> = {
                let local = local_node.read().await;
                let mut snapshots = vec![NodeSnapshot {
                    name: local.name.clone(),
                    mqtt_addr: local.mqtt_addr.to_string(),
                    cluster_addr: local.cluster_addr.to_string(),
                    assigned_groups: local.assigned_groups.iter().cloned().collect(),
                }];

                for peer in peers.iter() {
                    if peer.name != node_name {
                        snapshots.push(NodeSnapshot {
                            name: peer.name.clone(),
                            mqtt_addr: peer.mqtt_addr.to_string(),
                            cluster_addr: peer.cluster_addr.to_string(),
                            assigned_groups: peer.assigned_groups.iter().cloned().collect(),
                        });
                    }
                }

                snapshots
            };

            let assignments: Vec<GroupAssignment> = {
                let ga = group_assignments.read().await;
                ga.iter()
                    .map(|(group, node)| GroupAssignment {
                        group: group.clone(),
                        node_name: node.clone(),
                    })
                    .collect()
            };

            let response = ClusterMessage::JoinResponse {
                accepted: true,
                nodes,
                group_assignments: assignments,
            };

            let bytes = response.to_bytes();
            stream.write_all(&bytes).await?;
        }

        ClusterMessage::Heartbeat {
            node_name,
            mqtt_addr,
            assigned_groups,
            connection_count,
        } => {
            if let Some(mut peer) = peers.get_mut(&node_name) {
                peer.update_heartbeat(assigned_groups, connection_count);
            } else {
                // New peer discovered via heartbeat
                if let Ok(mqtt_socket) = mqtt_addr.parse() {
                    let peer_addr = stream.peer_addr()?;
                    let peer = NodeInfo::remote(node_name.clone(), peer_addr, mqtt_socket);
                    peers.insert(node_name, peer);
                }
            }
        }

        ClusterMessage::GroupAssigned { group, node_name } => {
            let mut assignments = group_assignments.write().await;
            assignments.insert(group.clone(), node_name.clone());
            tracing::info!("Group {} assigned to node {}", group, node_name);
        }

        ClusterMessage::GroupUnassigned {
            group,
            previous_node,
        } => {
            let mut assignments = group_assignments.write().await;
            if assignments.get(&group) == Some(&previous_node) {
                assignments.remove(&group);
                tracing::info!("Group {} unassigned from node {}", group, previous_node);
            }
        }

        ClusterMessage::RetainedSync {
            topic,
            payload,
            source_node,
        } => {
            tracing::debug!(
                "Received retained message sync from {}: {}",
                source_node,
                topic
            );
            let msg = RetainedMessage {
                topic,
                payload,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                source_node,
            };
            retained_store.store_synced(msg);
        }

        ClusterMessage::RetainedRequest { topic_pattern } => {
            tracing::debug!(
                "Received retained message request for pattern: {}",
                topic_pattern
            );
            let local = local_node.read().await;
            let source_node = local.name.clone();
            drop(local);

            // Send matching retained messages back
            let messages = retained_store.get_matching(&topic_pattern);
            for msg in messages {
                let sync_msg = ClusterMessage::RetainedSync {
                    topic: msg.topic,
                    payload: msg.payload,
                    source_node: source_node.clone(),
                };
                let bytes = sync_msg.to_bytes();
                if let Err(e) = stream.write_all(&bytes).await {
                    tracing::debug!("Failed to send retained message: {}", e);
                    break;
                }
            }
        }

        ClusterMessage::JoinResponse { .. } => {
            // JoinResponse is handled in connect_to_peer, not here
            tracing::debug!("Unexpected JoinResponse on incoming connection");
        }
    }

    Ok(())
}

/// Connect to a peer and join the cluster
async fn connect_to_peer(
    peer_addr: &str,
    peers: Arc<DashMap<String, NodeInfo>>,
    local_node: Arc<RwLock<NodeInfo>>,
) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect(peer_addr).await?;

    let local = local_node.read().await;
    let join_request = ClusterMessage::JoinRequest {
        node_name: local.name.clone(),
        mqtt_addr: local.mqtt_addr.to_string(),
        cluster_addr: local.cluster_addr.to_string(),
        assigned_groups: local.assigned_groups.clone(),
    };
    drop(local);

    // Send join request
    let bytes = join_request.to_bytes();
    stream.write_all(&bytes).await?;

    // Read response length
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    // Read response body
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let response = ClusterMessage::from_bytes(&body)?;

    if let ClusterMessage::JoinResponse {
        accepted, nodes, ..
    } = response
    {
        if accepted {
            tracing::info!("Successfully joined cluster, {} nodes known", nodes.len());

            for node in nodes {
                if let (Ok(cluster_addr), Ok(mqtt_addr)) =
                    (node.cluster_addr.parse(), node.mqtt_addr.parse())
                {
                    let peer = NodeInfo::remote_with_groups(
                        node.name.clone(),
                        cluster_addr,
                        mqtt_addr,
                        node.assigned_groups.into_iter().collect(),
                    );
                    peers.insert(node.name, peer);
                }
            }
        } else {
            tracing::warn!("Join request rejected by {}", peer_addr);
        }
    }

    Ok(())
}

/// Send a message to a specific peer
async fn send_message_to_peer(addr: &SocketAddr, message: &ClusterMessage) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect(addr).await?;
    let bytes = message.to_bytes();
    stream.write_all(&bytes).await?;
    Ok(())
}
