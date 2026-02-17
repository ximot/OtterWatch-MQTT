//! MQTT Broker module
//!
//! Provides the OtterWatch MQTT broker implementation based on rumqttd.
//! Includes custom authentication integration and OtterWatch-specific features.

mod core;

pub use core::{BrokerHandle, OtterWatchBroker};
