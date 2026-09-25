import subprocess
from pathlib import Path

import networkx as nx
import pytest

from graph_engine import build_graph, distances_to, map_ids

GCC_FLAGS = ["-shared", "-fPIC", "-O0", "-fno-inline"]

GOLDEN_ID_MAP = {0x114C: 100, 0x1171: 101, 0x1186: 102, 0x119F: 103}
GOLDEN_DISTANCES = {"100": 6, "101": 4, "102": 2, "103": 0}


def test_target_block_is_at_zero_distance():
    graph = nx.DiGraph([(1, 2), (2, 3)])
    assert distances_to(graph, 3) == {3: 0, 2: 1, 1: 2}


def test_shortest_path_wins_over_long_path():
    graph = nx.DiGraph([(1, 2), (2, 4), (1, 3), (3, 4), (4, 3)])
    assert distances_to(graph, 4)[1] == 2


def test_blocks_without_a_path_are_excluded():
    graph = nx.DiGraph([(1, 2), (9, 10)])
    assert distances_to(graph, 2) == {2: 0, 1: 1}


def test_input_graph_is_not_mutated():
    graph = nx.DiGraph([(1, 2), (2, 3)])
    edges_before = set(graph.edges())
    distances_to(graph, 3)
    assert set(graph.edges()) == edges_before


@pytest.fixture(scope="session")
def if_nest_so(tmp_path_factory):
    source = Path(__file__).parent.parent / "Tests" / "if_nest.c"
    out = tmp_path_factory.mktemp("golden") / "libif_nest.so"
    subprocess.run(["gcc", *GCC_FLAGS, str(source), "-o", str(out)], check=True)
    return out


@pytest.fixture(scope="session")
def if_nest_graph(if_nest_so):
    import angr

    project = angr.Project(
        str(if_nest_so), auto_load_libs=False, main_opts={"base_addr": 0}
    )
    cfg = project.analyses.CFGFast(normalize=True)
    func = cfg.functions.get(project.loader.find_symbol("target_function").rebased_addr)
    return project, build_graph(func)


def test_hit_block_ids_are_recovered(if_nest_graph):
    project, graph = if_nest_graph
    assert map_ids(project, graph) == GOLDEN_ID_MAP


def test_distances_lead_to_the_named_target(if_nest_graph):
    project, graph = if_nest_graph
    id_map = map_ids(project, graph)
    target_block = next(addr for addr, hit_id in id_map.items() if hit_id == 103)
    addr_distances = distances_to(graph, target_block)
    distances = {
        str(id_map[addr]): dist
        for addr, dist in addr_distances.items()
        if addr in id_map
    }
    assert distances == GOLDEN_DISTANCES
