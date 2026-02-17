//! OtterWatch MQTT Broker
//!
//! A custom MQTT broker optimized for the OtterWatch monitoring system.
//!
//! ## Features
//!
//! - Full MQTT 3.1.1 broker based on rumqttd
//! - API key authentication with O(1) lookup
//! - Prometheus metrics integration
//! - Management REST API with dashboard
//! - HA support via agent_group sharding (Phase 3)
//!
//! ## Usage
//!
//! ```ignore
//! use otterwatch_mqtt::{config, broker, api, metrics};
//!
//! let config = config::load_config(None)?;
//! let broker = broker::OtterWatchBroker::new(config);
//! let handle = broker.start().await?;
//! ```

pub mod api;
pub mod auth;
pub mod broker;
pub mod cluster;
pub mod config;
pub mod metrics;
pub mod proto;

pub use auth::ApiKeyValidator;
pub use broker::{BrokerHandle, OtterWatchBroker};
pub use config::{load_config, Config};
pub use metrics::MetricsCollector;
