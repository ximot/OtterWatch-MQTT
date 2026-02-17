#!/bin/bash
#
# OtterWatch MQTT Cluster Startup Script
# Starts multiple broker nodes for HA setup
#
# Usage:
#   ./start-cluster.sh [node_count]    # Start N nodes (default: 2)
#   ./start-cluster.sh stop            # Stop all nodes
#   ./start-cluster.sh status          # Show status
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CLUSTER_DIR="${PROJECT_DIR}/cluster"
BINARY="${PROJECT_DIR}/target/release/otterwatch-mqtt"
LOG_DIR="${CLUSTER_DIR}/logs"
PID_DIR="${CLUSTER_DIR}/pids"

# Default configuration
NODE_COUNT=${1:-2}
BASE_MQTT_PORT=1883
BASE_API_PORT=8085
BASE_METRICS_PORT=9090
BASE_CLUSTER_PORT=7000
API_KEY="test-secret-key-12345"
# Public IP for cluster (used for mqtt public_addr and cluster advertise)
# Override with: PUBLIC_IP=x.x.x.x ./start-cluster.sh
PUBLIC_IP="${PUBLIC_IP:-192.168.1.100}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Build if needed
build_binary() {
    if [[ ! -f "$BINARY" ]]; then
        log_info "Building release binary..."
        cd "$PROJECT_DIR"
        cargo build --release
        log_success "Build complete"
    fi
}

# Generate config for a node
generate_config() {
    local node_num=$1
    local node_name="node-${node_num}"
    local config_file="${CLUSTER_DIR}/${node_name}/settings.toml"
    local data_dir="${CLUSTER_DIR}/${node_name}/data"

    local mqtt_port=$((BASE_MQTT_PORT + node_num - 1))
    local api_port=$((BASE_API_PORT + node_num - 1))
    local metrics_port=$((BASE_METRICS_PORT + node_num - 1))
    local cluster_port=$((BASE_CLUSTER_PORT + node_num - 1))

    # Generate list of peer addresses (all nodes except self)
    local peers=""
    for i in $(seq 1 $NODE_COUNT); do
        if [[ $i -ne $node_num ]]; then
            local peer_port=$((BASE_CLUSTER_PORT + i - 1))
            if [[ -n "$peers" ]]; then
                peers="${peers}, "
            fi
            peers="${peers}\"127.0.0.1:${peer_port}\""
        fi
    done

    # Generate assigned groups based on node number
    local groups=""
    case $node_num in
        1) groups='["default", "web-servers"]' ;;
        2) groups='["databases", "monitoring"]' ;;
        3) groups='["cache", "api-servers"]' ;;
        *) groups='[]' ;;
    esac

    mkdir -p "${CLUSTER_DIR}/${node_name}" "$data_dir"

    cat > "$config_file" << EOF
# OtterWatch MQTT Broker - ${node_name}
# Auto-generated cluster configuration

[node]
name = "${node_name}"
data_dir = "${data_dir}"

[mqtt]
listen_addr = "0.0.0.0:${mqtt_port}"
public_addr = "${PUBLIC_IP}:${mqtt_port}"
max_connections = 10000
router_buffer_size = 50000
keepalive_secs = 30
max_packet_size = 262144
max_qos = 1

[auth]
api_keys = ["${API_KEY}"]
require_auth = true
api_key_source = "password"

[cluster]
enabled = true
bind_addr = "0.0.0.0:${cluster_port}"
assigned_groups = ${groups}

[cluster.discovery]
static_peers = [${peers}]
etcd_endpoints = []

[metrics]
enabled = true
listen_addr = "0.0.0.0:${metrics_port}"

[api]
listen_addr = "0.0.0.0:${api_port}"
dashboard_enabled = true
cors_origins = ["*"]
EOF
}

# Start a single node
start_node() {
    local node_num=$1
    local node_name="node-${node_num}"
    local config_file="${CLUSTER_DIR}/${node_name}/settings.toml"
    local log_file="${LOG_DIR}/${node_name}.log"
    local pid_file="${PID_DIR}/${node_name}.pid"

    # Check if already running
    if [[ -f "$pid_file" ]]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            log_warn "${node_name} is already running (PID: $pid)"
            return 0
        fi
    fi

    log_info "Starting ${node_name}..."

    # Start the broker
    RUST_LOG=info "$BINARY" -c "$config_file" > "$log_file" 2>&1 &
    local pid=$!
    echo $pid > "$pid_file"

    # Wait a bit and check if it's running
    sleep 1
    if kill -0 "$pid" 2>/dev/null; then
        local mqtt_port=$((BASE_MQTT_PORT + node_num - 1))
        local api_port=$((BASE_API_PORT + node_num - 1))
        log_success "${node_name} started (PID: $pid, MQTT: $mqtt_port, API: $api_port)"
    else
        log_error "${node_name} failed to start. Check ${log_file}"
        return 1
    fi
}

# Stop a single node
stop_node() {
    local node_num=$1
    local node_name="node-${node_num}"
    local pid_file="${PID_DIR}/${node_name}.pid"

    if [[ -f "$pid_file" ]]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            log_info "Stopping ${node_name} (PID: $pid)..."
            kill "$pid" 2>/dev/null || true

            # Wait for graceful shutdown
            local timeout=10
            while kill -0 "$pid" 2>/dev/null && [[ $timeout -gt 0 ]]; do
                sleep 1
                ((timeout--))
            done

            if kill -0 "$pid" 2>/dev/null; then
                log_warn "Force killing ${node_name}..."
                kill -9 "$pid" 2>/dev/null || true
            fi

            log_success "${node_name} stopped"
        else
            log_warn "${node_name} is not running"
        fi
        rm -f "$pid_file"
    else
        log_warn "${node_name} PID file not found"
    fi
}

# Show cluster status
show_status() {
    echo ""
    echo "===== OtterWatch MQTT Cluster Status ====="
    echo ""

    local running=0
    local total=0

    for pid_file in "${PID_DIR}"/*.pid; do
        [[ -f "$pid_file" ]] || continue

        local node_name=$(basename "$pid_file" .pid)
        local pid=$(cat "$pid_file")
        total=$((total + 1))

        if kill -0 "$pid" 2>/dev/null; then
            running=$((running + 1))
            echo -e "  ${GREEN}●${NC} ${node_name} (PID: $pid) - RUNNING"
        else
            echo -e "  ${RED}●${NC} ${node_name} (PID: $pid) - STOPPED"
        fi
    done

    if [[ $total -eq 0 ]]; then
        echo "  No nodes configured"
    fi

    echo ""
    echo "Summary: ${running}/${total} nodes running"
    echo ""

    # Show ports if nodes are configured
    if [[ -d "$CLUSTER_DIR" ]]; then
        echo "Node Ports:"
        for i in $(seq 1 5); do
            local config="${CLUSTER_DIR}/node-${i}/settings.toml"
            if [[ -f "$config" ]]; then
                local mqtt_port=$((BASE_MQTT_PORT + i - 1))
                local api_port=$((BASE_API_PORT + i - 1))
                local metrics_port=$((BASE_METRICS_PORT + i - 1))
                local cluster_port=$((BASE_CLUSTER_PORT + i - 1))
                echo "  node-${i}: MQTT=${mqtt_port} API=${api_port} Metrics=${metrics_port} Cluster=${cluster_port}"
            fi
        done
        echo ""
    fi
}

# Start all nodes
start_cluster() {
    log_info "Starting OtterWatch MQTT cluster with ${NODE_COUNT} nodes..."
    echo ""

    build_binary

    mkdir -p "$LOG_DIR" "$PID_DIR"

    # Generate all configs first
    log_info "Generating configurations..."
    for i in $(seq 1 $NODE_COUNT); do
        generate_config $i >/dev/null
        log_success "Generated config for node-${i}"
    done
    echo ""

    # Start all nodes
    for i in $(seq 1 $NODE_COUNT); do
        start_node $i
        # Small delay between nodes
        sleep 1
    done

    echo ""
    log_success "Cluster startup complete!"
    echo ""

    # Show connection info
    echo "===== Connection Information ====="
    echo ""
    for i in $(seq 1 $NODE_COUNT); do
        local mqtt_port=$((BASE_MQTT_PORT + i - 1))
        local api_port=$((BASE_API_PORT + i - 1))
        echo "node-${i}:"
        echo "  MQTT:       tcp://localhost:${mqtt_port}"
        echo "  API:        http://localhost:${api_port}"
        echo "  Health:     http://localhost:${api_port}/health"
        echo ""
    done

    echo "===== Server Configuration ====="
    echo ""
    echo "Add to otterwatch-server settings.toml:"
    echo ""
    echo -n 'mqtt_cluster_nodes = ['
    for i in $(seq 1 $NODE_COUNT); do
        local api_port=$((BASE_API_PORT + i - 1))
        if [[ $i -gt 1 ]]; then echo -n ", "; fi
        echo -n "\"http://localhost:${api_port}\""
    done
    echo ']'
    echo ""

    echo "===== Agent Configuration ====="
    echo ""
    echo "Agents can connect to any node. Example:"
    echo "  mqtt_broker_addr = \"tcp://localhost:${BASE_MQTT_PORT}\""
    echo ""
}

# Stop all nodes
stop_cluster() {
    log_info "Stopping OtterWatch MQTT cluster..."
    echo ""

    for pid_file in "${PID_DIR}"/*.pid; do
        [[ -f "$pid_file" ]] || continue
        local node_name=$(basename "$pid_file" .pid)
        local node_num=${node_name#node-}
        stop_node $node_num
    done

    echo ""
    log_success "Cluster stopped"
}

# Clean cluster data
clean_cluster() {
    log_info "Cleaning cluster data..."

    # Stop all nodes first
    stop_cluster 2>/dev/null || true

    rm -rf "$CLUSTER_DIR"
    log_success "Cluster data cleaned"
}

# Show logs
show_logs() {
    local node_name="${1:-node-1}"
    local log_file="${LOG_DIR}/${node_name}.log"

    if [[ -f "$log_file" ]]; then
        tail -f "$log_file"
    else
        log_error "Log file not found: $log_file"
        exit 1
    fi
}

# Main command handler
case "${1:-}" in
    stop)
        stop_cluster
        ;;
    status)
        show_status
        ;;
    clean)
        clean_cluster
        ;;
    logs)
        show_logs "${2:-node-1}"
        ;;
    restart)
        stop_cluster
        sleep 2
        NODE_COUNT=${2:-2}
        start_cluster
        ;;
    help|--help|-h)
        echo "OtterWatch MQTT Cluster Manager"
        echo ""
        echo "Usage: $0 [command] [options]"
        echo ""
        echo "Commands:"
        echo "  [N]         Start cluster with N nodes (default: 2)"
        echo "  stop        Stop all cluster nodes"
        echo "  status      Show cluster status"
        echo "  restart [N] Restart cluster with N nodes"
        echo "  clean       Stop and remove all cluster data"
        echo "  logs [node] Tail logs for a node (default: node-1)"
        echo "  help        Show this help"
        echo ""
        echo "Examples:"
        echo "  $0          # Start 2-node cluster"
        echo "  $0 3        # Start 3-node cluster"
        echo "  $0 stop     # Stop all nodes"
        echo "  $0 logs node-2  # Show logs for node-2"
        echo ""
        ;;
    *)
        if [[ "${1:-}" =~ ^[0-9]+$ ]]; then
            NODE_COUNT=$1
        fi
        start_cluster
        ;;
esac
