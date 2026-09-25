import argparse
import json
import sys

import angr
import networkx as nx


def main():
    parser = argparse.ArgumentParser(description="Talon CFG Distance Engine")
    parser.add_argument("binary", help="Path to target .so / ELF")
    parser.add_argument("target_symbol", help="Target function or block symbol name")
    parser.add_argument(
        "--out", default="distances.json", help="Path for the distance map JSON"
    )
    args = parser.parse_args()

    print(f"[+] Loading binary: {args.binary}")
    project = angr.Project(
        args.binary, auto_load_libs=False, main_opts={"base_addr": 0}
    )

    print("[+] Building complete CFGFast...")
    cfg = project.analyses.CFGFast(normalize=True)

    symbol = project.loader.find_symbol(args.target_symbol)
    if not symbol:
        sys.exit(f"[-] Symbol '{args.target_symbol}' not found")

    func_addr = symbol.rebased_addr
    print(f"[+] Function '{args.target_symbol}' entry address: {hex(func_addr)}")

    func = cfg.functions.get(func_addr)
    if not func:
        sys.exit(f"[-] Function at {hex(func_addr)} not analyzed in CFG")

    graph = nx.DiGraph()
    for block in func.blocks:
        graph.add_node(block.addr)
    for src, dst in func.graph.edges():
        graph.add_edge(src.addr, dst.addr)

    end_nodes = [node for node in graph.nodes() if graph.out_degree(node) == 0]
    if not end_nodes:
        sys.exit("[-] No end node found in the CFG")
    target_block_addr = end_nodes[0]

    print(f"[+] Selected target basic block: {hex(target_block_addr)}")

    distances = {}
    for node_addr in graph.nodes():
        try:
            length = nx.shortest_path_length(
                graph, source=node_addr, target=target_block_addr
            )
            distances[hex(node_addr)] = int(length)
        except nx.NetworkXNoPath:
            continue

    with open(args.out, "w") as f:
        json.dump(distances, f, indent=4)

    print(f"[+] Wrote {len(distances)} block distances to {args.out}")


if __name__ == "__main__":
    main()
