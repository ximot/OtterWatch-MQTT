//! Configuration loading from TOML files and environment variables

use crate::config::schema::Config;
use anyhow::{Context, Result};
use std::path::Path;

/// Load configuration from file and environment variables
///
/// Priority (highest to lowest):
/// 1. Environment variables (prefix: OTTERWATCH_MQTT_)
/// 2. Configuration file (settings.toml)
/// 3. Default values
pub fn load_config(config_path: Option<&Path>) -> Result<Config> {
    let mut builder = config::Config::builder();

    // Try to load from file
    let config_file = config_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("settings.toml"));

    if config_file.exists() {
        tracing::info!("Loading configuration from: {}", config_file.display());
        builder = builder.add_source(config::File::from(config_file.as_path()));
    } else if config_path.is_some() {
        // User explicitly specified a config file that doesn't exist
        anyhow::bail!("Configuration file not found: {}", config_file.display());
    } else {
        tracing::warn!(
            "No configuration file found at {}, using defaults",
            config_file.display()
        );
    }

    // Add environment variables with prefix OTTERWATCH_MQTT_
    // e.g., OTTERWATCH_MQTT_MQTT__LISTEN_ADDR -> mqtt.listen_addr
    builder = builder.add_source(
        config::Environment::with_prefix("OTTERWATCH_MQTT")
            .separator("__")
            .try_parsing(true),
    );

    let settings = builder.build().context("Failed to build configuration")?;

    let config: Config = settings
        .try_deserialize()
        .context("Failed to deserialize configuration")?;

    validate_config(&config)?;

    Ok(config)
}

/// Validate configuration values
fn validate_config(config: &Config) -> Result<()> {
    // Validate MQTT settings
    if config.mqtt.max_connections == 0 {
        anyhow::bail!("mqtt.max_connections must be greater than 0");
    }

    if config.mqtt.router_buffer_size == 0 {
        anyhow::bail!("mqtt.router_buffer_size must be greater than 0");
    }

    if config.mqtt.max_qos > 2 {
        anyhow::bail!("mqtt.max_qos must be 0, 1, or 2");
    }

    // Validate auth settings
    if config.auth.require_auth && config.auth.api_keys.is_empty() {
        anyhow::bail!("auth.require_auth is true but no api_keys are configured");
    }

    // Validate cluster settings
    if config.cluster.enabled {
        let discovery = &config.cluster.discovery;
        let has_discovery = !discovery.static_peers.is_empty()
            || discovery.dns_name.is_some()
            || !discovery.etcd_endpoints.is_empty();

        if !has_discovery {
            tracing::warn!(
                "Cluster is enabled but no discovery method configured. \
                 This node will run standalone."
            );
        }
    }

    Ok(())
}

/// Load configuration for testing
#[cfg(test)]
pub fn load_test_config() -> Config {
    Config {
        node: crate::config::schema::NodeConfig {
            name: "test-node".to_string(),
            data_dir: std::path::PathBuf::from("/tmp/otterwatch-mqtt-test"),
        },
        mqtt: crate::config::schema::MqttConfig::default(),
        auth: crate::config::schema::AuthConfig {
            api_keys: vec!["test-key".to_string()],
            require_auth: true,
            api_key_source: crate::config::schema::ApiKeySource::Password,
        },
        cluster: crate::config::schema::ClusterConfig::default(),
        metrics: crate::config::schema::MetricsConfig::default(),
        api: crate::config::schema::ApiConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[node]
name = "test-broker"
data_dir = "/tmp/test"

[mqtt]
listen_addr = "127.0.0.1:1883"
max_connections = 5000

[auth]
api_keys = ["key1", "key2"]
require_auth = true
"#
        )
        .unwrap();

        let config = load_config(Some(file.path())).unwrap();
        assert_eq!(config.node.name, "test-broker");
        assert_eq!(config.mqtt.listen_addr, "127.0.0.1:1883");
        assert_eq!(config.mqtt.max_connections, 5000);
        assert_eq!(config.auth.api_keys.len(), 2);
    }

    #[test]
    fn test_validation_no_api_keys() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[node]
name = "test"

[mqtt]

[auth]
require_auth = true
api_keys = []
"#
        )
        .unwrap();

        let result = load_config(Some(file.path()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no api_keys are configured"));
    }
}
