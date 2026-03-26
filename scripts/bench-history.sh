#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HISTORY_FILE="$REPO_ROOT/bench-history.csv"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
COMMIT=$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")

echo "Running benchmarks..."
cd "$REPO_ROOT"

# Run benchmarks and capture output
BENCH_OUTPUT=$(cargo bench 2>&1)

# Parse criterion output: extract "benchmark_name  time:   [low  mid  high]"
# We record the median (middle value).
parse_results() {
    local name=""
    while IFS= read -r line; do
        # Match benchmark name lines (indented or not, ending with "time:")
        if [[ "$line" =~ ^([a-zA-Z_][a-zA-Z0-9_ /\(\)]+)[[:space:]]+$ ]]; then
            name="${BASH_REMATCH[1]}"
            name="${name%"${name##*[![:space:]]}"}"  # trim trailing spaces
        fi
        if [[ "$line" =~ time:[[:space:]]+\[([0-9.]+)[[:space:]]+(ns|µs|ms|s)[[:space:]]+([0-9.]+)[[:space:]]+(ns|µs|ms|s)[[:space:]]+([0-9.]+)[[:space:]]+(ns|µs|ms|s)\] ]]; then
            local median="${BASH_REMATCH[3]}"
            local unit="${BASH_REMATCH[4]}"
            # Preceding line may have set the name
            if [[ -z "$name" ]]; then
                # Try to extract name from current line
                if [[ "$line" =~ ^([a-zA-Z_][a-zA-Z0-9_ /\(\)]+)[[:space:]]+time: ]]; then
                    name="${BASH_REMATCH[1]}"
                    name="${name%"${name##*[![:space:]]}"}"
                fi
            fi
            if [[ -n "$name" ]]; then
                echo "${TIMESTAMP},${COMMIT},${name},${median},${unit}"
            fi
            name=""
        fi
    done <<< "$BENCH_OUTPUT"
}

# Create header if file doesn't exist
if [ ! -f "$HISTORY_FILE" ]; then
    echo "timestamp,commit,benchmark,median,unit" > "$HISTORY_FILE"
fi

RESULTS=$(parse_results)
if [ -n "$RESULTS" ]; then
    echo "$RESULTS" >> "$HISTORY_FILE"
    echo ""
    echo "Appended $(echo "$RESULTS" | wc -l) results to $HISTORY_FILE"
    echo ""
    echo "$RESULTS" | column -t -s ','
else
    echo "WARNING: No benchmark results parsed."
    echo "Raw output saved for debugging."
fi
