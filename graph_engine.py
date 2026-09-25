import argparse
import json
import sys

import angr
import networkx as nx
from capstone import CS_ARCH_X86, CS_MODE_64, Cs
from capstone.x86 import X86_OP_IMM

MAP_SIZE = 4096
SCHEMA_VERSION = 1
HIT_BLOCK_SYMBOL = "hit_block"
HIT_BLOCK_ARG_REG = "edi"


def build_graph(func):
    graph = nx.DiGraph()
    for block in func.blocks:
        graph.add_node(block.addr)
    for src, dst in func.graph.edges():
        graph.add_edge(src.addr, dst.addr)
    return graph


def map_ids(project, graph):
    """Map the address of each basic block that calls hit_block(N) to N.

    An ID names the block containing the call, which is the block right
    before the branch or store that the ID is meant to name. The argument
    is recovered from the constant move into the first argument register
    that directly precedes the call inside the same block.
    """
    symbol = project.loader.find_symbol(HIT_BLOCK_SYMBOL)
    if not symbol:
        sys.exit(f"[-] Symbol '{HIT_BLOCK_SYMBOL}' not found")
    hit_block_addrs = {symbol.rebased_addr}
    plt_addr = project.loader.main_object.plt.get(HIT_BLOCK_SYMBOL)
    if plt_addr is not None:
        hit_block_addrs.add(plt_addr)

    disassembler = Cs(CS_ARCH_X86, CS_MODE_64)
    disassembler.detail = True

    id_map = {}
    for addr in graph.nodes():
        pending_arg = None
        for insn in disassembler.disasm(project.factory.block(addr).bytes, addr):
            if insn.mnemonic == "mov" and insn.op_str.startswith(
                f"{HIT_BLOCK_ARG_REG}, "
            ):
                if insn.operands[1].type == X86_OP_IMM:
                    pending_arg = insn.operands[1].imm
            elif (
                insn.mnemonic == "call"
                and pending_arg is not None
                and insn.operands[0].type == X86_OP_IMM
                and insn.operands[0].imm in hit_block_addrs
            ):
                id_map[addr] = pending_arg
    return id_map


def distances_to(graph, target_addr):
    """Hop counts from every block in graph to target_addr, in one BFS."""
    return nx.single_source_shortest_path_length(graph.reverse(copy=False), target_addr)


def main():
    parser = argparse.ArgumentParser(description="Talon CFG Distance Engine")
    parser.add_argument("binary", help="Path to target .so / ELF")
    parser.add_argument("target_symbol", help="Target function or block symbol name")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument(
        "--target-id", type=int, help="hit_block id whose call block is the target"
    )
    group.add_argument(
        "--target-addr", help="Basic block address to target, for example 0x11a9"
    )
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

    graph = build_graph(func)
    id_map = map_ids(project, graph)
    print(f"[+] Found {len(id_map)} hit_block calls: {sorted(id_map.values())}")

    if args.target_id is not None:
        blocks_for_id = [
            addr for addr, hit_id in id_map.items() if hit_id == args.target_id
        ]
        if not blocks_for_id:
            sys.exit(
                f"[-] No block in '{args.target_symbol}' calls hit_block({args.target_id})"
            )
        target_block_addr = blocks_for_id[0]
        target_hit_id = args.target_id
    else:
        try:
            target_block_addr = int(args.target_addr, 16)
        except ValueError:
            sys.exit(f"[-] Invalid target address: {args.target_addr}")
        if target_block_addr not in graph:
            sys.exit(
                f"[-] Target address {args.target_addr} is not a basic block of '{args.target_symbol}'"
            )
        target_hit_id = id_map.get(target_block_addr)

    print(f"[+] Target basic block: {hex(target_block_addr)}")

    addr_distances = distances_to(graph, target_block_addr)
    distances = {}
    for block_addr, dist in addr_distances.items():
        hit_id = id_map.get(block_addr)
        if hit_id is None:
            continue
        if not 0 <= hit_id < MAP_SIZE:
            sys.exit(
                f"[-] hit_block id {hit_id} is outside the signal map of size {MAP_SIZE}"
            )
        distances[str(hit_id)] = dist

    payload = {
        "version": SCHEMA_VERSION,
        "target_id": target_hit_id,
        "map_size": MAP_SIZE,
        "distances": dict(sorted(distances.items())),
    }

    with open(args.out, "w") as f:
        json.dump(payload, f, indent=4)

    print(f"[+] Wrote {len(distances)} block distances to {args.out}")


if __name__ == "__main__":
    main()
