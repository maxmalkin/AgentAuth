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

    if [ ! -f .env ]; then
        echo -e "${RED}Error: .env file not found. Copy .env.example to .env and fill in values.${RESET}"
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
        echo -e "${CYAN}Waiting for services to be ready...${RESET}"
        sleep 3
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

# Load environment
set -a
source .env
set +a

ensure_docker

# Build Rust binaries first so startup is fast
echo -e "${CYAN}Building Rust binaries...${RESET}"
cargo build -p registry-bin -p verifier-bin 2>&1 | prefix_output "$CYAN" "build"

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

echo -e "${BOLD}${CYAN}"
echo "  ┌──────────────────────────────────────┐"
echo "  │  Registry:    http://localhost:${REGISTRY_PORT:-8080}    │"
echo "  │  Verifier:    http://localhost:${VERIFIER_PORT:-8081}    │"
echo "  │  Approval UI: http://localhost:${PORT:-3001}    │"
echo "  │                                      │"
echo "  │  Press Ctrl+C to stop all services   │"
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
