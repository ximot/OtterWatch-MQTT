//! HTTP API module for management and metrics
//!
//! Provides REST endpoints for:
//! - Health checks
//! - Prometheus metrics
//! - Connection management
//! - Cluster status (when enabled)

use crate::cluster::ClusterCoordinator;
use crate::config::Config;
use crate::metrics::MetricsCollector;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Shared state for API handlers
#[derive(Clone)]
pub struct ApiState {
    pub config: Arc<Config>,
    pub metrics: MetricsCollector,
    pub cluster: Arc<ClusterCoordinator>,
    pub start_time: std::time::Instant,
}

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub node: String,
    pub uptime_secs: u64,
}

/// Server statistics
#[derive(Serialize)]
pub struct StatsResponse {
    pub node: String,
    pub uptime_secs: u64,
    pub connections_active: i64,
    pub subscriptions_active: i64,
    pub messages_received: u64,
    pub messages_failed: u64,
}

/// Create the API router
pub fn create_router(state: ApiState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/stats", get(stats))
        .route("/api/connections", get(connections))
        .route("/api/cluster/nodes", get(cluster_nodes))
        .route("/api/cluster/groups", get(cluster_groups))
        .route(
            "/api/cluster/retained",
            get(cluster_retained).post(cluster_retained_store),
        )
        .route("/api/cluster/rebalance", post(cluster_rebalance))
        .layer(cors)
        .with_state(state)
}

/// Health check endpoint
async fn health(State(state): State<ApiState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();
    Json(HealthResponse {
        status: "ok",
        node: state.config.node.name.clone(),
        uptime_secs: uptime,
    })
}

/// Prometheus metrics endpoint
async fn metrics(State(state): State<ApiState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        state.metrics.gather(),
    )
}

/// Server statistics endpoint
async fn stats(State(state): State<ApiState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();
    Json(StatsResponse {
        node: state.config.node.name.clone(),
        uptime_secs: uptime,
        connections_active: state.metrics.connections_active.get(),
        subscriptions_active: state.metrics.subscriptions_active.get(),
        messages_received: state.metrics.messages_received.get(),
        messages_failed: state.metrics.messages_failed.get(),
    })
}

/// Connection summary response
#[derive(Serialize)]
pub struct ConnectionsResponse {
    pub count: i64,
    pub subscriptions: i64,
}

/// Connections list endpoint
/// Returns connection count (per-client details require rumqttd extension)
async fn connections(State(state): State<ApiState>) -> impl IntoResponse {
    Json(ConnectionsResponse {
        count: state.metrics.connections_active.get(),
        subscriptions: state.metrics.subscriptions_active.get(),
    })
}

/// Cluster nodes endpoint
async fn cluster_nodes(State(state): State<ApiState>) -> impl IntoResponse {
    let nodes = state.cluster.get_nodes().await;
    Json(serde_json::json!({
        "enabled": state.cluster.is_enabled(),
        "nodes": nodes,
    }))
}

/// Cluster group assignments endpoint
async fn cluster_groups(State(state): State<ApiState>) -> impl IntoResponse {
    let assignments = state.cluster.get_group_assignments().await;
    Json(serde_json::json!({
        "enabled": state.cluster.is_enabled(),
        "assignments": assignments,
    }))
}

/// Cluster retained messages endpoint
async fn cluster_retained(State(state): State<ApiState>) -> impl IntoResponse {
    let messages = state.cluster.get_retained_messages();
    Json(serde_json::json!({
        "enabled": state.cluster.is_enabled(),
        "count": messages.len(),
        "messages": messages,
    }))
}

/// Request to store a retained message
#[derive(Deserialize)]
pub struct StoreRetainedRequest {
    pub topic: String,
    pub payload: String,
}

/// Store a retained message and sync to cluster
async fn cluster_retained_store(
    State(state): State<ApiState>,
    Json(req): Json<StoreRetainedRequest>,
) -> impl IntoResponse {
    let payload = req.payload.into_bytes();
    state
        .cluster
        .store_retained(req.topic.clone(), payload)
        .await;

    Json(serde_json::json!({
        "status": "ok",
        "topic": req.topic,
    }))
}

/// Request for reassigning a group to a different node
#[derive(Deserialize)]
pub struct RebalanceRequest {
    /// Group to reassign
    pub group: String,
    /// Target node name
    pub node: String,
}

/// Response for rebalance operation
#[derive(Serialize)]
pub struct RebalanceResponse {
    pub success: bool,
    pub message: String,
    pub group: String,
    pub node: String,
}

/// Reassign a group to a different node
async fn cluster_rebalance(
    State(state): State<ApiState>,
    Json(req): Json<RebalanceRequest>,
) -> impl IntoResponse {
    if !state.cluster.is_enabled() {
        return (
            StatusCode::BAD_REQUEST,
            Json(RebalanceResponse {
                success: false,
                message: "Cluster mode is not enabled".to_string(),
                group: req.group,
                node: req.node,
            }),
        );
    }

    let changed = state
        .cluster
        .reassign_group(req.group.clone(), req.node.clone())
        .await;

    if changed {
        (
            StatusCode::OK,
            Json(RebalanceResponse {
                success: true,
                message: format!("Group '{}' reassigned to node '{}'", req.group, req.node),
                group: req.group,
                node: req.node,
            }),
        )
    } else {
        (
            StatusCode::OK,
            Json(RebalanceResponse {
                success: false,
                message: format!(
                    "No change - group '{}' is already assigned to '{}' or node not found",
                    req.group, req.node
                ),
                group: req.group,
                node: req.node,
            }),
        )
    }
}

/// Start the API server
pub async fn start_server(state: ApiState, listen_addr: &str) -> anyhow::Result<()> {
    let router = create_router(state);
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    tracing::info!("API server listening on {}", listen_addr);

    axum::serve(listener, router).await?;

    Ok(())
}
