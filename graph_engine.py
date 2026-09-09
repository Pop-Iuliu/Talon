import argparse
import json
import angr
import networkx as nx

def main():
    parser = argparse.ArgumentParser(description="Talon CFG Distance Engine")
    parser.add_argument("binary", help="Path to target .so / ELF")
    parser.add_argument("target_symbol", help="Target function or block symbol name")
    args = parser.parse_args()

    print(f"[+] Loading binary: {args.binary}")
    p = angr.Project(args.binary, auto_load_libs=False, main_opts={'base_addr': 0})

    print("[+] Building complete CFGFast...")
    cfg = p.analyses.CFGFast(normalize=True)

    symbol = p.loader.find_symbol(args.target_symbol)
    if not symbol:
        print(f"[-] Error: Symbol '{args.target_symbol}' not found!")
        return

    func_addr = symbol.rebased_addr
    print(f"[+] Function '{args.target_symbol}' entry address: {hex(func_addr)}")

    func = cfg.functions.get(func_addr)
    if not func:
        print(f"[-] Error: Function at {hex(func_addr)} not analyzed in CFG!")
        return

    pure_graph = nx.DiGraph()
    for block in func.blocks:
        pure_graph.add_node(block.addr)

    for src, dst in func.graph.edges():
        pure_graph.add_edge(src.addr, dst.addr)

    end_nodes = [node for node in pure_graph.nodes() if pure_graph.out_degree(node) == 0]
    target_block_addr = end_nodes[0] if end_nodes else max(pure_graph.nodes())

    print(f"[+] Selected target basic block inside function: {hex(target_block_addr)}")

    distances = {}
    for node_addr in pure_graph.nodes():
        try:
            length = nx.shortest_path_length(pure_graph, source=node_addr, target=target_block_addr)
            distances[hex(node_addr)] = int(length)
        except nx.NetworkXNoPath:
            continue

    with open("distances.json", "w") as f:
        json.dump(distances, f, indent=4)

    print(f"[+] Successfully wrote {len(distances)} block distances to distances.json")

if __name__ == "__main__":
    main()