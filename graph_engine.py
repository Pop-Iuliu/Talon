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

    target_addr = symbol.rebased_addr
    print(f"[+] Target symbol '{args.target_symbol}' address: {hex(target_addr)}")

    target_node = cfg.model.get_any_node(target_addr)
    if not target_node:
        nodes = [n for n in cfg.graph.nodes() if hasattr(n, 'addr') and n.addr <= target_addr < n.addr + n.size]
        target_node = nodes[0] if nodes else None

    if not target_node:
        print(f"[-] Error: Target block at {hex(target_addr)} not found in CFG!")
        return

    distances = {}

    G = nx.DiGraph(cfg.graph)

    for node in cfg.graph.nodes():
        if not hasattr(node, 'addr'):
            continue
        try:
            length = nx.shortest_path_length(G, source=node, target=target_node)
            distances[hex(node.addr)] = int(length)
        except (nx.NetworkXNoPath, nx.NodeNotFound):
            continue

    with open("distances.json", "w") as f:
        json.dump(distances, f, indent=4)

    print(f"[+] Successfully wrote {len(distances)} block distances to distances.json")

if __name__ == "__main__":
    main()