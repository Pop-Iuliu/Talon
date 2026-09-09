#!/bin/bash
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m'
fuser -k 1337/tcp 2>/dev/null || true
pkill -9 -f dgf_core 2>/dev/null || true

clear

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
gcc -shared -fPIC Tests/if_nest.c -o Tests/libif_nest.so
if [ $? -eq 0 ]; then
    echo -e "${GREEN}  [✔] Target compiled successfully (libif_nest.so)${NC}\n"
else
    echo -e "${RED}  [✘] Compilation failed!${NC}"
    exit 1
fi

echo -e "${YELLOW}[2/3] Extracting CFG Distances...${NC}"
python graph_engine.py Tests/libif_nest.so target_function
if [ $? -eq 0 ]; then
    echo -e "${GREEN}  [✔] Distance extraction complete${NC}\n"
else
    echo -e "${RED}  [✘] Graph Engine failed!${NC}"
    exit 1
fi

echo -e "${YELLOW}[3/3] Launching LibAFL Engine...${NC}"
export LD_LIBRARY_PATH="$(pwd)/Tests:$LD_LIBRARY_PATH"

cd Fuzzer && cargo run