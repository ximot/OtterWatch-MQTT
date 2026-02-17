//! Authentication module for otterwatch-mqtt broker
//!
//! Provides API key validation for MQTT client connections.
//! Uses SHA256 hashing with HashSet for O(1) lookup performance.

mod api_key;

pub use api_key::ApiKeyValidator;
