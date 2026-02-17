# OtterWatch-MQTT Documentation

Custom MQTT broker for the OtterWatch monitoring system - a high-performance, Rust-based drop-in replacement for Mosquitto.

## Features

- **High Performance** - Handles 10,000+ concurrent connections
- **OtterWatch Optimized** - Topic routing tailored for OtterWatch agents
- **Prometheus Metrics** - Built-in monitoring endpoint
- **REST API** - Management interface for health checks and statistics
- **HA Ready** - Designed for agent_group sharding (Phase 3)
- **Simple Configuration** - TOML config with environment variable overrides

## Quick Start

### Installation

```bash
# Build from source
git clone https://github.com/ximot/otterwatch-mqtt
cd otterwatch-mqtt
cargo build --release

# Binary location
./target/release/otterwatch-mqtt
```

### Configuration

Create `settings.toml`:

```toml
[node]
name = "otterwatch-mqtt-1"

[mqtt]
listen_addr = "0.0.0.0:1883"
max_connections = 10000
keepalive_secs = 30

[auth]
# Use the same API keys as otterwatch-server
api_keys = ["your-secret-api-key"]
require_auth = true

[metrics]
enabled = true
listen_addr = "0.0.0.0:9090"

[api]
listen_addr = "0.0.0.0:8085"
```

### Running

```bash
# Default config (settings.toml)
./otterwatch-mqtt

# Custom config file
./otterwatch-mqtt -c /etc/otterwatch/mqtt.toml

# With environment overrides
RUST_LOG=debug ./otterwatch-mqtt
```

## Migrating from Mosquitto

1. **Stop Mosquitto:**
   ```bash
   systemctl stop mosquitto
   ```

2. **Configure otterwatch-mqtt:**
   - Copy API keys from otterwatch-server settings
   - Set listen address (default: 0.0.0.0:1883)

3. **Start otterwatch-mqtt:**
   ```bash
   ./otterwatch-mqtt
   ```

4. **Update clients (if needed):**
   - Agents: Update `mqtt_broker_addr` in settings.toml
   - Server: Update `mqtt_broker_addr` in settings.toml

No other changes required - otterwatch-mqtt is protocol-compatible.

## Configuration Reference

### [node]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `name` | string | hostname | Unique node identifier |
| `data_dir` | path | `./data` | Directory for persistent data |

### [mqtt]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `listen_addr` | string | `0.0.0.0:1883` | MQTT listen address |
| `max_connections` | integer | 10000 | Maximum concurrent connections |
| `router_buffer_size` | integer | 50000 | Internal message queue size |
| `keepalive_secs` | integer | 30 | Client keepalive timeout |
| `max_packet_size` | integer | 262144 | Maximum MQTT packet size (bytes) |
| `max_qos` | integer | 1 | Maximum QoS level (0, 1, or 2) |

### [auth]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `api_keys` | array | [] | List of valid API keys |
| `require_auth` | bool | true | Require authentication |
| `api_key_source` | string | `password` | Where to find API key in CONNECT |

### [metrics]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | bool | true | Enable Prometheus metrics |
| `listen_addr` | string | `0.0.0.0:9090` | Metrics endpoint address |

### [api]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `listen_addr` | string | `0.0.0.0:8085` | API endpoint address |
| `dashboard_enabled` | bool | true | Enable web dashboard |
| `cors_origins` | array | ["*"] | Allowed CORS origins |

### [cluster] (Phase 3)

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | bool | false | Enable cluster mode |
| `bind_addr` | string | `0.0.0.0:7000` | Cluster communication address |
| `assigned_groups` | array | [] | Groups assigned to this node |

## Environment Variables

Override any config value with environment variables:

```bash
# Prefix: OTTERWATCH_MQTT_
# Separator: __ (double underscore)

# Examples:
export OTTERWATCH_MQTT_MQTT__LISTEN_ADDR="0.0.0.0:1883"
export OTTERWATCH_MQTT_AUTH__API_KEYS="key1,key2"
export OTTERWATCH_MQTT_METRICS__ENABLED="true"

# Logging
export RUST_LOG=info                    # info, debug, trace
export RUST_LOG=rumqttd=debug           # Debug only rumqttd
```

## API Reference

### Health Check

```bash
curl http://localhost:8085/health
```

Response:
```json
{
  "status": "ok",
  "node": "otterwatch-mqtt-1",
  "uptime_secs": 3600
}
```

### Statistics

```bash
curl http://localhost:8085/api/stats
```

Response:
```json
{
  "node": "otterwatch-mqtt-1",
  "uptime_secs": 3600,
  "connections_active": 150,
  "messages_received": 45000,
  "messages_sent": 45000
}
```

### Prometheus Metrics

```bash
curl http://localhost:9090/metrics
```

Response:
```prometheus
# HELP otterwatch_mqtt_connections_active Active MQTT connections
# TYPE otterwatch_mqtt_connections_active gauge
otterwatch_mqtt_connections_active 150

# HELP otterwatch_mqtt_messages_received_total Total messages received
# TYPE otterwatch_mqtt_messages_received_total counter
otterwatch_mqtt_messages_received_total 45000
```

## Systemd Service

Create `/etc/systemd/system/otterwatch-mqtt.service`:

```ini
[Unit]
Description=OtterWatch MQTT Broker
After=network.target

[Service]
Type=simple
User=otterwatch
Group=otterwatch
ExecStart=/usr/local/bin/otterwatch-mqtt -c /etc/otterwatch/mqtt.toml
Restart=always
RestartSec=5

# Security
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/otterwatch-mqtt

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable otterwatch-mqtt
sudo systemctl start otterwatch-mqtt
```

## Troubleshooting

### Port Already in Use

```
Error: Address already in use (os error 98)
```

Solution: Stop Mosquitto or change port:
```bash
sudo systemctl stop mosquitto
# or
# Change listen_addr in settings.toml
```

### Agent Disconnects (Keepalive)

```
ERROR disconnected error=Network(KeepAlive(Elapsed(())))
```

Solution: Increase keepalive timeout in both broker and agent:
```toml
# Broker settings.toml
[mqtt]
keepalive_secs = 60

# Agent settings.toml
mqtt_keepalive_secs = 60
```

### No Metrics Data

If agents are connected but no data appears in dashboard:
1. Check agent logs for publish errors
2. Verify server is subscribed to correct topics
3. Check topic prefix matches (`otterwatch/metrics`)

### Debug Logging

```bash
RUST_LOG=debug ./otterwatch-mqtt
```

## Performance Tuning

### System Limits

For high connection counts, increase file descriptor limits:

```bash
# /etc/security/limits.conf
otterwatch soft nofile 65535
otterwatch hard nofile 65535

# Or for systemd service
# Add to [Service] section:
LimitNOFILE=65535
```

### Recommended Settings for 4000+ Agents

```toml
[mqtt]
max_connections = 10000
router_buffer_size = 100000
keepalive_secs = 60

[node]
data_dir = "/var/lib/otterwatch-mqtt"
```

## Version History

### v0.1.0 (Phase 1)
- Initial release
- Drop-in Mosquitto replacement
- Basic metrics and API endpoints
- TOML configuration with env overrides

## License

MIT License - See LICENSE file for details.
