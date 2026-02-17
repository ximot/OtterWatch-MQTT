//! Core MQTT broker implementation wrapping rumqttd

use crate::auth::ApiKeyValidator;
use crate::config::Config;
use anyhow::Result;
use rumqttd::{
    meters::MetersLink, BridgeConfig as RumqttdBridgeConfig, Broker, Config as RumqttdConfig,
    ConnectionSettings, MetricSettings, MetricType, RouterConfig, ServerSettings,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use tokio::sync::broadcast;

/// OtterWatch MQTT Broker
///
/// Wraps rumqttd with custom authentication and OtterWatch-specific features
pub struct OtterWatchBroker {
    config: Config,
    api_key_validator: Arc<ApiKeyValidator>,
    shutdown_tx: broadcast::Sender<()>,
}

impl OtterWatchBroker {
    /// Create a new broker instance
    pub fn new(config: Config) -> Self {
        let api_key_validator = Arc::new(ApiKeyValidator::new(
            &config.auth.api_keys,
            config.auth.require_auth,
        ));

        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config,
            api_key_validator,
            shutdown_tx,
        }
    }

    /// Get API key validator for external use
    pub fn api_key_validator(&self) -> Arc<ApiKeyValidator> {
        self.api_key_validator.clone()
    }

    /// Get shutdown signal sender
    pub fn shutdown_sender(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Build rumqttd configuration from our config
    fn build_rumqttd_config(&self) -> RumqttdConfig {
        let listen_addr: SocketAddr = self
            .config
            .mqtt
            .listen_addr
            .parse()
            .expect("Invalid mqtt.listen_addr");

        // Router configuration
        let router = RouterConfig {
            max_connections: self.config.mqtt.max_connections,
            max_outgoing_packet_count: self.config.mqtt.router_buffer_size as u64,
            max_segment_size: self.config.mqtt.max_packet_size,
            max_segment_count: 10, // At least 1 segment must exist in memory
            initialized_filters: None,
            ..Default::default()
        };

        // Connection settings
        // keepalive_secs * 1.5 * 1000 = timeout in ms (MQTT spec allows 1.5x keepalive)
        // For 30s keepalive: 30 * 1500 + 5000 = 50000ms (50s timeout)
        let keepalive_timeout_ms =
            ((self.config.mqtt.keepalive_secs as u32) * 1500 + 5000).min(65000) as u16;

        let connection = ConnectionSettings {
            connection_timeout_ms: keepalive_timeout_ms,
            max_payload_size: self.config.mqtt.max_packet_size,
            max_inflight_count: 100,
            auth: None,
            dynamic_filters: false,
            external_auth: None,
        };

        // Server settings (TCP without TLS)
        let server = ServerSettings {
            name: self.config.node.name.clone(),
            listen: listen_addr,
            tls: None,
            next_connection_delay_ms: 1,
            connections: connection,
        };

        let mut servers = HashMap::new();
        servers.insert("v4".to_string(), server);

        // Note: rumqttd's built-in PrometheusSetting would conflict with our custom
        // metrics endpoint on the same port. For now we disable it and use our own.
        let prometheus = None;

        // Configure metrics push interval (1 second) so rumqttd sends metrics to MetersLink
        // Note: rumqttd has a bug where both Meters AND Alerts must be configured
        // because the timer select! uses unwrap() before checking is_some()
        let meters_settings: MetricSettings =
            toml::from_str("push_interval = 1").expect("Invalid metric settings");
        let alerts_settings: MetricSettings =
            toml::from_str("push_interval = 60").expect("Invalid metric settings");
        let mut metrics_config = HashMap::new();
        metrics_config.insert(MetricType::Meters, meters_settings);
        metrics_config.insert(MetricType::Alerts, alerts_settings);

        // Build bridge configuration if any bridges are configured
        // Note: rumqttd only supports one bridge at a time, so we use the first one
        let bridge = if let Some(bridge_cfg) = self.config.bridge.first() {
            let bridge_connection = ConnectionSettings {
                connection_timeout_ms: ((bridge_cfg.ping_delay_secs as u32) * 1500 + 5000)
                    .min(65000) as u16,
                max_payload_size: self.config.mqtt.max_packet_size,
                max_inflight_count: 100,
                auth: None,
                dynamic_filters: false,
                external_auth: None,
            };

            tracing::info!(
                "Configuring bridge '{}' to {} with topic '{}'",
                bridge_cfg.name,
                bridge_cfg.addr,
                bridge_cfg.topic
            );

            if self.config.bridge.len() > 1 {
                tracing::warn!(
                    "Multiple bridges configured but rumqttd only supports one. Using first bridge '{}'",
                    bridge_cfg.name
                );
            }

            Some(RumqttdBridgeConfig {
                name: bridge_cfg.name.clone(),
                addr: bridge_cfg.addr.clone(),
                qos: bridge_cfg.qos,
                sub_path: bridge_cfg.topic.clone(),
                reconnection_delay: bridge_cfg.reconnection_delay_secs,
                ping_delay: bridge_cfg.ping_delay_secs,
                connections: bridge_connection,
                transport: Default::default(),
            })
        } else {
            None
        };

        RumqttdConfig {
            id: 0,
            router,
            v4: Some(servers),
            v5: None,
            ws: None,
            cluster: None,
            console: None,
            prometheus,
            bridge,
            metrics: Some(metrics_config),
        }
    }

    /// Start the MQTT broker
    ///
    /// This spawns the broker in a separate thread and returns a handle for management
    /// along with a MetersLink for receiving metrics updates.
    pub fn start(self) -> Result<(BrokerHandle, MetersLink)> {
        let rumqttd_config = self.build_rumqttd_config();
        let node_name = self.config.node.name.clone();
        let listen_addr = self.config.mqtt.listen_addr.clone();

        tracing::info!(
            "Starting OtterWatch MQTT broker '{}' on {}",
            node_name,
            listen_addr
        );

        // Create the broker
        let broker = Broker::new(rumqttd_config);

        // Get meters link BEFORE starting the broker (must be done before start() which is blocking)
        let meters_link = broker
            .meters()
            .map_err(|e| anyhow::anyhow!("Failed to get meters link: {}", e))?;

        // Spawn broker in background thread (broker.start() is blocking)
        let broker_thread = thread::spawn(move || {
            let mut broker = broker;
            if let Err(e) = broker.start() {
                tracing::error!("Broker error: {}", e);
            }
        });

        tracing::info!("MQTT broker started successfully");

        Ok((
            BrokerHandle {
                config: self.config,
                api_key_validator: self.api_key_validator,
                shutdown_tx: self.shutdown_tx,
                _broker_thread: broker_thread,
            },
            meters_link,
        ))
    }
}

/// Handle to a running broker
pub struct BrokerHandle {
    pub config: Config,
    pub api_key_validator: Arc<ApiKeyValidator>,
    shutdown_tx: broadcast::Sender<()>,
    _broker_thread: thread::JoinHandle<()>,
}

impl BrokerHandle {
    /// Signal the broker to shut down
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// Wait for the broker to exit (blocking)
    pub async fn wait(self) {
        // Since broker runs in its own thread, we just wait for signals
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let _ = shutdown_rx.recv().await;
    }
}
