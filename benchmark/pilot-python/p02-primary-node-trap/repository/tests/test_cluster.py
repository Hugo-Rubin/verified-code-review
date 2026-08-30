"""Tests for cluster membership and status. Run with: python -m pytest"""

import sys
import pathlib

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "src"))

from cluster import Cluster, ClusterError, Node  # noqa: E402
from status import all_healthy, status_report  # noqa: E402


def test_rejects_an_empty_cluster():
    with pytest.raises(ClusterError):
        Cluster([])


def test_replace_preserves_the_node_count():
    c = Cluster([Node("a")])
    assert c.replace(0, Node("b"))
    assert len(c.nodes()) == 1
    assert not c.replace(9, Node("c"))


def test_reports_the_first_node_as_primary():
    c = Cluster([Node("a"), Node("b")])
    report = status_report(c)
    assert report["primary"] == "a"
    assert report["total"] == 2
    assert all_healthy(report)


def test_counts_unhealthy_nodes():
    c = Cluster([Node("a"), Node("b", healthy=False)])
    assert status_report(c)["unhealthy"] == 1
