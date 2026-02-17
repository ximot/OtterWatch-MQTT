//! Metrics module for Prometheus integration
//!
//! Collects metrics from rumqttd's MetersLink and exposes them via Prometheus.

use prometheus::{IntCounter, IntGauge, Registry};
use rumqttd::meters::MetersLink;
use rumqttd::Meter;
use std::sync::Arc;

/// Metrics collector for the MQTT broker
#[derive(Clone)]
pub struct MetricsCollector {
    registry: Arc<Registry>,
    pub connections_active: IntGauge,
    pub subscriptions_active: IntGauge,
    pub messages_received: IntCounter,
    pub messages_failed: IntCounter,
    pub bytes_received: IntCounter,
    pub bytes_sent: IntCounter,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let registry = Registry::new();

        let connections_active = IntGauge::new(
            "otterwatch_mqtt_connections_active",
            "Number of active MQTT connections",
        )
        .unwrap();

        let subscriptions_active = IntGauge::new(
            "otterwatch_mqtt_subscriptions_active",
            "Number of active subscriptions",
        )
        .unwrap();

        let messages_received = IntCounter::new(
            "otterwatch_mqtt_messages_received_total",
            "Total MQTT messages received (publishes)",
        )
        .unwrap();

        let messages_failed = IntCounter::new(
            "otterwatch_mqtt_messages_failed_total",
            "Total failed MQTT publishes",
        )
        .unwrap();

        let bytes_received = IntCounter::new(
            "otterwatch_mqtt_bytes_received_total",
            "Total bytes received",
        )
        .unwrap();

        let bytes_sent =
            IntCounter::new("otterwatch_mqtt_bytes_sent_total", "Total bytes sent").unwrap();

        registry
            .register(Box::new(connections_active.clone()))
            .unwrap();
        registry
            .register(Box::new(subscriptions_active.clone()))
            .unwrap();
        registry
            .register(Box::new(messages_received.clone()))
            .unwrap();
        registry
            .register(Box::new(messages_failed.clone()))
            .unwrap();
        registry.register(Box::new(bytes_received.clone())).unwrap();
        registry.register(Box::new(bytes_sent.clone())).unwrap();

        Self {
            registry: Arc::new(registry),
            connections_active,
            subscriptions_active,
            messages_received,
            messages_failed,
            bytes_received,
            bytes_sent,
        }
    }

    /// Get the Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Generate Prometheus text format output
    pub fn gather(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a background task that polls the MetersLink and updates the MetricsCollector
pub fn spawn_metrics_updater(metrics: MetricsCollector, meters_link: MetersLink) {
    std::thread::Builder::new()
        .name("metrics-updater".to_string())
        .spawn(move || {
            tracing::info!("Metrics updater thread started");
            loop {
                // Non-blocking receive of metrics
                match meters_link.recv() {
                    Ok(meters) => {
                        // Count subscriptions from Meter::Subscription events
                        // since RouterMeter.total_subscriptions is not tracked by rumqttd
                        let mut subscription_count = 0;
                        for meter in &meters {
                            match meter {
                                Meter::Router(_id, router_meter) => {
                                    metrics
                                        .connections_active
                                        .set(router_meter.total_connections as i64);
                                    // These are deltas since last poll, so we increment
                                    metrics
                                        .messages_received
                                        .inc_by(router_meter.total_publishes as u64);
                                    metrics
                                        .messages_failed
                                        .inc_by(router_meter.failed_publishes as u64);
                                }
                                Meter::Subscription(_filter, _sub_meter) => {
                                    subscription_count += 1;
                                }
                            }
                        }
                        // Update subscription count if we received subscription meters
                        if subscription_count > 0 {
                            metrics.subscriptions_active.set(subscription_count);
                        }
                    }
                    Err(e) => {
                        // TryRecvError::Empty is expected when no data, just continue
                        if !matches!(e, rumqttd::meters::LinkError::TryRecv(_)) {
                            tracing::warn!("Metrics link error: {}", e);
                        }
                    }
                }
                // Small sleep to avoid busy-looping
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
        .expect("Failed to spawn metrics updater thread");
}
