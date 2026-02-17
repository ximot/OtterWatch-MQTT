//! OtterWatch MQTT Broker
//!
//! Custom MQTT broker optimized for OtterWatch monitoring system.
//! Drop-in replacement for Mosquitto with better scalability.

use anyhow::Result;
use otterwatch_mqtt::{
    api::{self, ApiState},
    broker::OtterWatchBroker,
    cluster::ClusterCoordinator,
    config::load_config,
    metrics::{spawn_metrics_updater, MetricsCollector},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;

/// Command line arguments
struct Args {
    config_path: Option<PathBuf>,
}

impl Args {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut config_path = None;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-c" | "--config" => {
                    i += 1;
                    if i < args.len() {
                        config_path = Some(PathBuf::from(&args[i]));
                    }
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "-v" | "--version" => {
                    println!("otterwatch-mqtt {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                _ => {
                    eprintln!("Unknown argument: {}", args[i]);
                    print_help();
                    std::process::exit(1);
                }
            }
            i += 1;
        }

        Self { config_path }
    }
}

fn print_help() {
    println!(
        r#"OtterWatch MQTT Broker {}

USAGE:
    otterwatch-mqtt [OPTIONS]

OPTIONS:
    -c, --config <FILE>    Path to configuration file [default: settings.toml]
    -h, --help             Print help information
    -v, --version          Print version information

ENVIRONMENT VARIABLES:
    OTTERWATCH_MQTT_*      Override config values (e.g., OTTERWATCH_MQTT_MQTT__LISTEN_ADDR)
    RUST_LOG               Set log level (e.g., info, debug, trace)

EXAMPLES:
    otterwatch-mqtt                        # Use default settings.toml
    otterwatch-mqtt -c /etc/mqtt.toml      # Use custom config file
"#,
        env!("CARGO_PKG_VERSION")
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("otterwatch_mqtt=info".parse()?)
                .add_directive("rumqttd=info".parse()?),
        )
        .init();

    // Parse command line arguments
    let args = Args::parse();

    // Load configuration
    let config = load_config(args.config_path.as_deref())?;
    let config_arc = Arc::new(config.clone());

    tracing::info!(
        "OtterWatch MQTT Broker v{} starting...",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!("Node: {}", config.node.name);
    tracing::info!("MQTT listen: {}", config.mqtt.listen_addr);
    tracing::info!("API listen: {}", config.api.listen_addr);
    tracing::info!(
        "Auth: {} ({} keys)",
        if config.auth.require_auth {
            "required"
        } else {
            "disabled"
        },
        config.auth.api_keys.len()
    );
    tracing::info!(
        "Cluster: {}",
        if config.cluster.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );

    // Create metrics collector
    let metrics = MetricsCollector::new();

    // Create cluster coordinator
    // Use public_addr if set, otherwise fall back to listen_addr
    let mqtt_addr_str = config
        .mqtt
        .public_addr
        .as_ref()
        .unwrap_or(&config.mqtt.listen_addr);
    let mqtt_addr = mqtt_addr_str
        .parse()
        .expect("Invalid mqtt.public_addr or mqtt.listen_addr");
    let cluster = Arc::new(if config.cluster.enabled {
        ClusterCoordinator::new(config.cluster.clone(), config.node.name.clone(), mqtt_addr)
    } else {
        ClusterCoordinator::disabled()
    });

    // Create and start the MQTT broker
    let broker = OtterWatchBroker::new(config.clone());
    let shutdown_tx = broker.shutdown_sender();
    let (broker_handle, meters_link) = broker.start()?;

    // Spawn metrics updater thread to poll rumqttd metrics
    spawn_metrics_updater(metrics.clone(), meters_link);

    // Start cluster coordinator if enabled
    if cluster.is_enabled() {
        let cluster_clone = cluster.clone();
        tokio::spawn(async move {
            if let Err(e) = cluster_clone.start().await {
                tracing::error!("Cluster coordinator error: {}", e);
            }
        });
    }

    // Create API state
    let api_state = ApiState {
        config: config_arc.clone(),
        metrics: metrics.clone(),
        cluster: cluster.clone(),
        start_time: std::time::Instant::now(),
    };

    // Start API server in background
    let api_listen = config.api.listen_addr.clone();
    let _api_handle = tokio::spawn(async move {
        if let Err(e) = api::start_server(api_state, &api_listen).await {
            tracing::error!("API server error: {}", e);
        }
    });

    // Start metrics endpoint if enabled and different from API
    let _metrics_handle =
        if config.metrics.enabled && config.metrics.listen_addr != config.api.listen_addr {
            let metrics_listen = config.metrics.listen_addr.clone();
            let metrics_clone = metrics.clone();
            Some(tokio::spawn(async move {
                // Simple metrics-only server
                use axum::{routing::get, Router};
                let app = Router::new().route(
                    "/metrics",
                    get(move || {
                        let m = metrics_clone.clone();
                        async move { m.gather() }
                    }),
                );

                let listener = tokio::net::TcpListener::bind(&metrics_listen)
                    .await
                    .expect("Failed to bind metrics endpoint");

                tracing::info!("Metrics endpoint listening on {}", metrics_listen);

                axum::serve(listener, app).await.ok();
            }))
        } else {
            None
        };

    tracing::info!("OtterWatch MQTT Broker is ready");

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, shutting down...");
        }
        _ = broker_handle.wait() => {
            tracing::info!("Broker stopped");
        }
    }

    // Cleanup
    let _ = shutdown_tx.send(());

    // Give tasks time to cleanup
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    tracing::info!("Shutdown complete");

    Ok(())
}
