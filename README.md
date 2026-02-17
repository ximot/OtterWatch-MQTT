# OtterWatch MQTT Broker

> Custom MQTT broker optimized for OtterWatch monitoring system

**Version:** 0.1.0  
**License:** MIT  
**Author:** Tomasz Wyderka

## 📋 Overview

OtterWatch MQTT Broker is a high-performance MQTT 3.1.1 broker built on rumqttd, specifically optimized for the OtterWatch monitoring ecosystem. It serves as a drop-in replacement for Mosquitto with additional features tailored for metrics collection.

### Key Features

✅ **High Performance**
- Built on rumqttd - async Rust MQTT broker
- Handles 10,000+ concurrent connections
- Large message buffers for burst traffic
- Efficient Protocol Buffers message parsing

✅ **Security & Authentication**
- API key authentication with SHA2 hashing
- Per-client authorization
- Configurable auth source (username or password field)

✅ **Monitoring & Management**
- REST API for broker management
- Prometheus metrics export
- Web dashboard for live monitoring
- Connection statistics and diagnostics

✅ **High Availability (Optional)**
- Multi-broker clustering with etcd
- Agent group assignment for load balancing
- Static peer discovery or DNS SRV records
- Automatic failover

✅ **OtterWatch Integration**
- Protocol Buffers message validation
- Optimized topic structure for metrics
- Compatible with OtterWatch agent and server

---

## 🚀 Quick Start

### Docker Deployment (Recommended)

```bash
# Using OtterWatch centralized build scripts
cd /path/to/otterwatch/scripts

# Build MQTT broker image
./build-mqtt.sh

# Start full stack (includes MQTT broker)
docker-compose -f docker-compose-full.yml up -d

# Or start only MQTT broker
docker run -d \
  -p 1883:1883 \
  -p 8085:8085 \
  -p 9090:9090 \
  -v otterwatch-mqtt-data:/app/data \
  -e OTTERWATCH_MQTT_AUTH__API_KEYS='["your-api-key"]' \
  localhost:5000/otterwatch-mqtt:latest
```

### Manual Installation

#### Prerequisites

```bash
# Ubuntu/Debian
sudo apt-get install build-essential pkg-config libssl-dev
```

#### Build and Run

```bash
# Clone and build
git clone https://github.com/ximot/otterwatch-mqtt.git
cd otterwatch-mqtt
cargo build --release

# Copy and edit configuration
cp settings.toml.example settings.toml
nano settings.toml

# Run
./target/release/otterwatch-mqtt

# Or with custom config
./target/release/otterwatch-mqtt -c /etc/otterwatch-mqtt/settings.toml
```

---

## ⚙️ Configuration

Configuration file: `settings.toml`

```toml
[node]
# Unique node name (defaults to hostname)
name = "otterwatch-mqtt-1"
# Directory for persistent data
data_dir = "./data"

[mqtt]
# MQTT listen address
listen_addr = "0.0.0.0:1883"
# Maximum concurrent connections
max_connections = 10000
# Router buffer size for message queuing
router_buffer_size = 50000
# Client keepalive timeout in seconds
keepalive_secs = 30
# Maximum packet size (256KB)
max_packet_size = 262144
# Maximum QoS level (0, 1, or 2)
max_qos = 1

[auth]
# Valid API keys (same keys as otterwatch-server)
api_keys = ["your-secret-api-key-here"]
# Whether authentication is required
require_auth = true
# Where to find API key: "password" or "username"
api_key_source = "password"

[cluster]
# Enable cluster mode for HA
enabled = false
# Bind address for cluster communication
bind_addr = "0.0.0.0:7000"
# Groups assigned to this node (empty = accept all)
assigned_groups = []

[cluster.discovery]
# Static peer addresses
static_peers = ["192.168.1.101:7000", "192.168.1.102:7000"]
# DNS name for SRV record discovery (optional)
# dns_name = "_mqtt._tcp.otterwatch.local"
# etcd endpoints (requires cluster-etcd feature)
etcd_endpoints = []

[metrics]
# Enable Prometheus metrics
enabled = true
# Metrics endpoint listen address
listen_addr = "0.0.0.0:9090"

[api]
# Management API listen address
listen_addr = "0.0.0.0:8085"
# Enable web dashboard
dashboard_enabled = true
# CORS allowed origins
cors_origins = ["*"]
```

### Environment Variables

Override with `OTTERWATCH_MQTT_` prefix:

```bash
OTTERWATCH_MQTT_MQTT__LISTEN_ADDR="0.0.0.0:1883"
OTTERWATCH_MQTT_AUTH__API_KEYS='["key1","key2"]'
OTTERWATCH_MQTT_AUTH__REQUIRE_AUTH="true"
OTTERWATCH_MQTT_METRICS__ENABLED="true"
OTTERWATCH_MQTT_API__LISTEN_ADDR="0.0.0.0:8085"
RUST_LOG=info  # trace, debug, info, warn, error
```

---

## 🔌 Management API

Base URL: `http://localhost:8085/api`

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/status` | GET | Broker status and statistics |
| `/clients` | GET | List connected clients |
| `/clients/{id}` | GET | Get specific client info |
| `/clients/{id}/disconnect` | POST | Disconnect a client |
| `/subscriptions` | GET | List all active subscriptions |
| `/metrics` | GET | Internal metrics (JSON) |

### Examples

```bash
# Health check
curl http://localhost:8085/health

# Broker status
curl http://localhost:8085/api/status | jq

# List connected clients
curl http://localhost:8085/api/clients | jq

# Disconnect a client
curl -X POST http://localhost:8085/api/clients/client-id/disconnect
```

---

## 📊 Prometheus Metrics

Metrics endpoint: `http://localhost:9090/metrics`

### Available Metrics

```
# Connection metrics
mqtt_connections_total           # Total connections
mqtt_connections_active          # Currently active connections
mqtt_connections_rejected        # Rejected connections (auth failures)

# Message metrics
mqtt_messages_received_total     # Total messages received
mqtt_messages_sent_total         # Total messages sent
mqtt_bytes_received_total        # Total bytes received
mqtt_bytes_sent_total            # Total bytes sent

# Topic metrics
mqtt_subscriptions_total         # Total active subscriptions
mqtt_topics_active               # Number of active topics

# Performance metrics
mqtt_message_process_duration    # Message processing time (histogram)
mqtt_queue_size                  # Router queue size
```

### Scraping with Prometheus

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'otterwatch-mqtt'
    static_configs:
      - targets: ['localhost:9090']
```

---

## 🎯 Authentication

### API Key Authentication

Clients authenticate using API keys matching those in `otterwatch-server`:

**Method 1: Password field**
```bash
mosquitto_pub -h localhost -p 1883 \
  -t 'otterwatch/test' \
  -m 'hello' \
  -u '' \
  -P 'your-api-key-here'
```

**Method 2: Username field** (set `api_key_source = "username"`)
```bash
mosquitto_pub -h localhost -p 1883 \
  -t 'otterwatch/test' \
  -m 'hello' \
  -u 'your-api-key-here' \
  -P ''
```

### API Key Hashing

API keys are hashed using SHA2-256 for secure storage:
```rust
use sha2::{Sha256, Digest};

let key_hash = format!("{:x}", Sha256::digest(api_key.as_bytes()));
```

---

## 🏗️ Architecture

### Components

```
┌─────────────────────────────────────────────────────────┐
│              OtterWatch MQTT Broker                      │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ MQTT         │→ │ Router       │→ │ Pub/Sub      │  │
│  │ Protocol     │  │ (rumqttd)    │  │ Engine       │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│         ↓                                       ↑        │
│  ┌──────────────┐                     ┌──────────────┐  │
│  │ Auth         │                     │ Persistence  │  │
│  │ (API Keys)   │                     │ (Optional)   │  │
│  └──────────────┘                     └──────────────┘  │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Management   │  │ Prometheus   │  │ Cluster      │  │
│  │ API          │  │ Metrics      │  │ Coordinator  │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### Source Modules

- `api/` - HTTP management API
- `auth/` - API key authentication
- `broker/` - Core MQTT broker logic (rumqttd wrapper)
- `cluster/` - Multi-broker clustering
- `config/` - Configuration management
- `metrics/` - Prometheus metrics collection
- `proto/` - Protocol Buffers definitions

---

## 🔄 Clustering (High Availability)

Enable clustering for load balancing and failover:

### Configuration

**Node 1:**
```toml
[node]
name = "mqtt-node-1"

[cluster]
enabled = true
bind_addr = "0.0.0.0:7000"
assigned_groups = ["web-servers", "databases"]

[cluster.discovery]
static_peers = ["mqtt-node-2:7000", "mqtt-node-3:7000"]
```

**Node 2:**
```toml
[node]
name = "mqtt-node-2"

[cluster]
enabled = true
bind_addr = "0.0.0.0:7000"
assigned_groups = ["monitoring", "logs"]

[cluster.discovery]
static_peers = ["mqtt-node-1:7000", "mqtt-node-3:7000"]
```

### Agent Configuration

Agents automatically discover and failover to available brokers:

```toml
# Agent settings.toml
mqtt_broker_addrs = [
  "tcp://mqtt-node-1:1883",
  "tcp://mqtt-node-2:1883",
  "tcp://mqtt-node-3:1883"
]
```

Or use bootstrap:
```toml
mqtt_bootstrap_url = "http://otterwatch-server:8080/api/cluster/bootstrap"
```

Server returns available brokers based on agent group.

---

## 🐳 Docker Deployment

### Standalone

```bash
docker run -d \
  --name otterwatch-mqtt \
  -p 1883:1883 \
  -p 8085:8085 \
  -p 9090:9090 \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/settings.toml:/app/settings.toml:ro \
  localhost:5000/otterwatch-mqtt:latest
```

### With Docker Compose

```yaml
services:
  mqtt:
    image: localhost:5000/otterwatch-mqtt:latest
    ports:
      - "1883:1883"
      - "8085:8085"
      - "9090:9090"
    environment:
      OTTERWATCH_MQTT_AUTH__API_KEYS: '["your-api-key"]'
      RUST_LOG: info
    volumes:
      - mqtt-data:/app/data
    restart: unless-stopped

volumes:
  mqtt-data:
```

### Building Image

```bash
# Using centralized script
cd /path/to/otterwatch/scripts
./build-mqtt.sh

# Or manually
cd /path/to/otterwatch-mqtt
docker build -t otterwatch-mqtt:latest .
```

---

## 🔧 Development

### Build Commands

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run with debug logging
RUST_LOG=debug cargo run

# Run tests
cargo test

# Code formatting
cargo fmt

# Linting
cargo clippy
```

### With Clustering Feature

```bash
# Build with etcd clustering support
cargo build --release --features cluster-etcd
```

---

## 🧪 Testing

### Test MQTT Connection

```bash
# Subscribe to all topics
mosquitto_sub -h localhost -p 1883 \
  -t '#' \
  -u '' \
  -P 'your-api-key' \
  -v

# Publish test message
mosquitto_pub -h localhost -p 1883 \
  -t 'test/topic' \
  -m 'Hello MQTT' \
  -u '' \
  -P 'your-api-key'
```

### Test Management API

```bash
# Health check
curl http://localhost:8085/health

# Broker status
curl http://localhost:8085/api/status | jq

# Connected clients
curl http://localhost:8085/api/clients | jq
```

### Load Testing

```bash
# Using mosquitto_pub in parallel
for i in {1..100}; do
  mosquitto_pub -h localhost -p 1883 \
    -t "test/load/$i" \
    -m "message $i" \
    -u '' -P 'your-api-key' &
done
wait
```

---

## 🛠️ Troubleshooting

### Connection refused

```bash
# Check if broker is running
netstat -tlnp | grep 1883

# Check logs
journalctl -u otterwatch-mqtt -f

# Test with telnet
telnet localhost 1883
```

### Authentication failures

```bash
# Verify API key in config
cat settings.toml | grep api_keys

# Check auth logs
RUST_LOG=debug ./otterwatch-mqtt 2>&1 | grep auth

# Test with correct key
mosquitto_pub -h localhost -p 1883 \
  -t 'test' -m 'test' \
  -u '' -P 'correct-api-key' \
  -d  # Debug mode
```

### High memory usage

```toml
# Reduce buffer sizes in settings.toml
[mqtt]
router_buffer_size = 10000  # Default: 50000
max_connections = 1000      # Default: 10000
```

### Message loss

```toml
# Increase QoS level
[mqtt]
max_qos = 1  # Or 2 for exactly-once delivery

# Increase buffer
router_buffer_size = 100000
```

---

## 📈 Performance Tuning

### System Limits

```bash
# Increase open file limit
ulimit -n 65535

# For systemd service
echo "LimitNOFILE=65535" >> /etc/systemd/system/otterwatch-mqtt.service
systemctl daemon-reload
```

### Configuration Tuning

```toml
[mqtt]
# For high-throughput scenarios
router_buffer_size = 100000
max_connections = 20000
max_packet_size = 1048576  # 1MB

# For low-latency scenarios
router_buffer_size = 10000
keepalive_secs = 10
```

---

## 🔒 Security Best Practices

1. **Change default API keys** in production
2. **Use TLS/SSL** for encrypted transport (configure in rumqttd)
3. **Restrict network access** using firewall rules
4. **Monitor failed auth attempts** via metrics
5. **Rotate API keys** periodically
6. **Use strong, random keys** (min 32 characters)

---

## 📝 License

MIT License - see [LICENSE](LICENSE) file

---

## 🔗 Links

- **GitHub:** https://github.com/ximot/otterwatch-mqtt
- **Agent:** https://github.com/ximot/otterwatch
- **Server:** https://github.com/ximot/otterwatch-server
- **AI Agent:** https://github.com/ximot/otterwatch-ai

---

**Part of the OtterWatch monitoring ecosystem**
