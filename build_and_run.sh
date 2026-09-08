#!/usr/bin/env bash
set -e

ROOT_DIR=$(pwd)
TESTS_DIR="$ROOT_DIR/Tests"
FUZZER_DIR="$ROOT_DIR/Fuzzer"

echo "=========================================="
echo "[+] Starting Talon Pipeline Orchestration"
echo "=========================================="

echo "[1/4] Compiling C target (if_nest.c) -> libif_nest.so..."
gcc -shared -fPIC -g -o "$TESTS_DIR/libif_nest.so" "$TESTS_DIR/if_nest.c"
cp "$TESTS_DIR/libif_nest.so" "$FUZZER_DIR/"

echo "[2/4] Extracting CFG Distances via graph_engine.py..."
python3 "$ROOT_DIR/graph_engine.py" "$TESTS_DIR/libif_nest.so" "target_function"

if [ -f "$ROOT_DIR/distances.json" ]; then
    cp "$ROOT_DIR/distances.json" "$FUZZER_DIR/distances.json"
fi

echo "[3/4] Launching Talon Directed Fuzzer..."
echo "=========================================="
export LD_LIBRARY_PATH="$TESTS_DIR:$FUZZER_DIR:$LD_LIBRARY_PATH"

cd "$FUZZER_DIR"
cargo run