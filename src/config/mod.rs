//! Configuration module for otterwatch-mqtt broker
//!
//! Handles loading and validating configuration from:
//! - TOML configuration file (settings.toml)
//! - Environment variables (prefix: OTTERWATCH_MQTT_)

pub mod loader;
pub mod schema;

pub use loader::load_config;
pub use schema::*;
