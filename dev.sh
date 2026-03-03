#!/usr/bin/env bash
set -euo pipefail

# ── Colors ──────────────────────────────────────────────────────────
BLUE='\033[0;34m'
GREEN='\033[0;32m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
RED='\033[0;31m'
BOLD='\033[1m'
RESET='\033[0m'

# ── PIDs to clean up ───────────────────────────────────────────────
PIDS=()

cleanup() {
    echo ""
    echo -e "${CYAN}${BOLD}Shutting down all services...${RESET}"
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid" 2>/dev/null || true
        fi
    done
    # Wait up to 5 seconds for clean exit
    for pid in "${PIDS[@]}"; do
        local i=0
        while kill -0 "$pid" 2>/dev/null && [ $i -lt 50 ]; do
            sleep 0.1
            i=$((i + 1))
        done
        # Force kill if still running
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
        fi
    done
    echo -e "${CYAN}All services stopped.${RESET}"
    exit 0
}

trap cleanup SIGINT SIGTERM

# ── Prerequisite checks ────────────────────────────────────────────
check_prereqs() {
    local missing=0

    if ! command -v cargo &>/dev/null; then
        echo -e "${RED}Error: cargo not found. Install Rust via rustup.${RESET}"
        missing=1
    fi

    if ! command -v bun &>/dev/null; then
        echo -e "${RED}Error: bun not found. Install bun: https://bun.sh${RESET}"
        missing=1
    fi

    if ! command -v sqlx &>/dev/null; then
        echo -e "${RED}Error: sqlx-cli not found. Install via: cargo install sqlx-cli${RESET}"
        missing=1
    fi

    if [ $missing -ne 0 ]; then
        exit 1
    fi
}

# ── Docker-compose check ───────────────────────────────────────────
ensure_docker() {
    if ! docker compose ps --status running 2>/dev/null | grep -q "postgres\|redis"; then
        echo -e "${CYAN}Docker services not running. Starting docker-compose...${RESET}"
        docker compose up -d
        echo -e "${CYAN}Waiting for PostgreSQL to be ready...${RESET}"
        local retries=0
        until docker compose exec -T postgres-primary pg_isready -U agentauth -q 2>/dev/null; do
            retries=$((retries + 1))
            if [ $retries -ge 30 ]; then
                echo -e "${RED}PostgreSQL not ready after 30s. Check docker-compose logs.${RESET}"
                exit 1
            fi
            sleep 1
        done
        echo -e "${GREEN}PostgreSQL is ready.${RESET}"
    else
        echo -e "${GREEN}Docker services already running.${RESET}"
    fi
}

# ── Prefix helper ──────────────────────────────────────────────────
# Reads stdin line-by-line, prefixes with colored tag
prefix_output() {
    local color="$1"
    local tag="$2"
    while IFS= read -r line; do
        echo -e "${color}[${tag}]${RESET} ${line}"
    done
}

# ── Main ────────────────────────────────────────────────────────────
cd "$(dirname "$0")"

echo -e "${BOLD}${CYAN}"
echo "  ╔══════════════════════════════════════╗"
echo "  ║         AgentAuth Dev Runner         ║"
echo "  ╚══════════════════════════════════════╝"
echo -e "${RESET}"

check_prereqs

# Load .env if it exists (config.toml is the primary config source)
if [ -f .env ]; then
    set -a
    source .env
    set +a
fi

ensure_docker

# Run database migrations
echo -e "${CYAN}Running database migrations...${RESET}"
DATABASE_URL="postgres://agentauth:agentauth_dev@localhost:5434/agentauth" \
    sqlx migrate run --source migrations 2>&1 | prefix_output "$CYAN" "migrate"

# Build Rust binaries first so startup is fast
echo -e "${CYAN}Building Rust binaries...${RESET}"
cargo build -p registry-bin -p verifier-bin -p demo-agent 2>&1 | prefix_output "$CYAN" "build"

echo ""
echo -e "${BOLD}Starting services...${RESET}"
echo ""

# Start registry
cargo run -p registry-bin 2>&1 | prefix_output "$BLUE" "registry" &
PIDS+=($!)

# Start verifier
cargo run -p verifier-bin 2>&1 | prefix_output "$GREEN" "verifier" &
PIDS+=($!)

# Start approval UI
(cd services/approval-ui && bun run dev) 2>&1 | prefix_output "$MAGENTA" "approval" &
PIDS+=($!)

# Start demo agent (waits for registry internally)
YELLOW='\033[0;33m'
(sleep 5 && cargo run -p demo-agent) 2>&1 | prefix_output "$YELLOW" "demo-agent" &
PIDS+=($!)

echo -e "${BOLD}${CYAN}"
echo "  ┌──────────────────────────────────────┐"
echo "  │  Registry:    http://localhost:${REGISTRY_PORT:-8080}    │"
echo "  │  Verifier:    http://localhost:${VERIFIER_PORT:-8081}    │"
echo "  │  Approval UI: http://localhost:${PORT:-3001}    │"
echo "  │  Mock Service: http://localhost:9090  │"
echo "  │  Grafana:     http://localhost:3000    │"
echo "  │  Demo Agent:  running                 │"
echo "  │                                       │"
echo "  │  Press Ctrl+C to stop all services    │"
echo "  └──────────────────────────────────────┘"
echo -e "${RESET}"

# Wait for any child to exit — if one crashes, report it
while true; do
    for i in "${!PIDS[@]}"; do
        pid="${PIDS[$i]}"
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" 2>/dev/null
            exit_code=$?
            if [ $exit_code -ne 0 ]; then
                echo -e "${RED}${BOLD}A service exited with code ${exit_code}. Shutting down...${RESET}"
                cleanup
            fi
        fi
    done
    sleep 1
done
