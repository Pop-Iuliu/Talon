#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${CYAN}"
cat << "EOF"
 _____  ___   _      _____  _   _
|_   _|/ _ \ | |    |  _  || \ | |
  | | / /_\ \| |    | | | ||  \| |
  | | |  _  || |____| |_| || |\  |
  \_/ \_| |_/\_____/\_____/\_| \_/
     Directed Greybox Fuzzer
EOF
echo -e "${NC}"

echo -e "${BLUE}[*] Starting Talon Pipeline Orchestration...${NC}\n"

echo -e "${YELLOW}[1/3] Compiling target...${NC}"
gcc -shared -fPIC -O0 -fno-inline Tests/if_nest.c -o Tests/libif_nest.so
echo -e "${GREEN}  [ok] Target compiled: Tests/libif_nest.so${NC}\n"

echo -e "${YELLOW}[2/3] Extracting CFG distances...${NC}"
${PYTHON:-python3} graph_engine.py Tests/libif_nest.so target_function --target-id 103 --out Fuzzer/distances.json
echo -e "${GREEN}  [ok] Distances written to Fuzzer/distances.json${NC}\n"

echo -e "${YELLOW}[3/3] Launching LibAFL engine...${NC}"
cd Fuzzer && cargo run --release
