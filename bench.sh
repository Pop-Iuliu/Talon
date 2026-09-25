#!/bin/bash
set -euo pipefail
export LC_ALL=C

cd "$(dirname "$0")"

TRIALS="${TRIALS:-10}"
TIMEOUT_SECS="${TIMEOUT_SECS:-60}"
RESULTS="${RESULTS:-bench_results.csv}"

echo "[*] Building the hard target and its distance map..."
gcc -shared -fPIC -O0 -fno-inline Tests/if_nest_hard.c -o Tests/libif_nest.so
${PYTHON:-python3} graph_engine.py Tests/libif_nest.so target_function --target-id 200 --out Fuzzer/distances.json

echo "[*] Building the fuzzer in release mode..."
(cd Fuzzer && cargo build --release)

echo "scheduler,trial,tte_secs,found" > "$RESULTS"

for sched in directed queue rand; do
    for trial in $(seq 1 "$TRIALS"); do
        rm -rf Fuzzer/crashes
        mkdir -p Fuzzer/crashes
        start=$(date +%s.%N)
        set +e
        timeout "$TIMEOUT_SECS" ./Fuzzer/target/release/dgf_core \
            --scheduler "$sched" --seed "$trial" \
            --cooling-secs 15 \
            --distances Fuzzer/distances.json --crashes-dir Fuzzer/crashes \
            > /dev/null 2>&1
        status=$?
        set -e
        end=$(date +%s.%N)
        tte=$(awk -v s="$start" -v e="$end" 'BEGIN {printf "%.3f", e - s}')
        if [ "$status" -eq 139 ]; then
            found=true
        else
            found=false
            tte="$TIMEOUT_SECS"
        fi
        printf '%s,%s,%s,%s\n' "$sched" "$trial" "$tte" "$found" >> "$RESULTS"
        echo "[$sched] trial $trial: tte=${tte}s found=$found"
    done
done

echo "[*] Results written to $RESULTS"
${PYTHON:-python3} tools/analyze_bench.py "$RESULTS"
