import angr
import networkx as nx
import json
import argparse
import sys

def main():
    parser = argparse.ArgumentParser(description="Sprint 1: Static Graph Engine for DGF")
    parser.add_argument("binary", help="Path to the target binary")
    parser.add_argument("target", help="Target basic block address (hex)", type=lambda x: int(x, 16))
    args = parser.parse_args()

    print(f"[+] Loading binary: {args.binary}")
    p = angr.Project(args.binary, load_options={'auto_load_libs': False})

    print("[+] Generating Inter-procedural CFG (CFGFast)...")
    cfg = p.analyses.CFGFast(normalize=True)

    if hasattr(cfg.model.graph, "to_networkx"):
        graph = cfg.model.graph.to_networkx()
    else:
        graph = cfg.graph.to_networkx()

    target_node = cfg.model.get_any_node(args.target, anyaddr=True)
    if not target_node:
        print(f"[-] Error: Target address {hex(args.target)} not found in CFG.")
        sys.exit(1)

    print(f"[+] Target node found ({hex(target_node.addr)}). Calculating distances...")

    reversed_graph = graph.reverse()

    try:
        lengths = nx.single_source_shortest_path_length(reversed_graph, target_node)
    except Exception as e:
        print(f"[-] Error calculating paths: {e}")
        sys.exit(1)

    distances = {}
    for node, dist in lengths.items():
        if node.addr:
            addr_str = hex(node.addr)
            if addr_str not in distances or dist < distances[addr_str]:
                distances[addr_str] = dist

    with open("distances.json", "w") as f:
        json.dump(distances, f, indent=4)
    print(f"[+] Saved distances for {len(distances)} basic blocks to distances.json")

    main_sym = p.loader.main_object.get_symbol("main")
    if main_sym:
        main_node = cfg.model.get_any_node(main_sym.rebased_addr, anyaddr=True)
        if main_node and main_node in lengths:
            dist_from_main = lengths[main_node]
            print(f"\n[+] --- Path Analysis ---")
            print(f"[+] Distance from main() to target: {dist_from_main} blocks")
            try:
                path = nx.shortest_path(graph, main_node, target_node)
                print("[+] Shortest path sequence:")
                for n in path:
                    func_name = n.name if hasattr(n, 'name') and n.name else "unknown"
                    print(f"    -> {hex(n.addr)} (Func: {func_name})")
            except nx.NetworkXNoPath:
                print("[-] No direct path found from main() to target.")
        else:
            print("[-] main() cannot reach the target block.")

if __name__ == "__main__":
    main()