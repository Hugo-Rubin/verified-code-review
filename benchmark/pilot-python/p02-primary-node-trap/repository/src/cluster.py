"""Cluster membership."""


class ClusterError(Exception):
    """Raised when a cluster cannot be constructed."""


class Node:
    def __init__(self, name, healthy=True):
        self.name = name
        self.healthy = healthy


class Cluster:
    """A set of nodes.

    Invariant: ``_nodes`` is never empty. ``Cluster`` is the only way to build
    one and its constructor rejects an empty sequence, the attribute is
    private, and no method removes a node. Callers may index ``nodes()`` at 0
    without checking.
    """

    def __init__(self, nodes):
        if not nodes:
            raise ClusterError("a cluster needs at least one node")
        self._nodes = list(nodes)

    def nodes(self):
        """The cluster's nodes. Never empty."""
        return list(self._nodes)

    def replace(self, index, node):
        """Swap one node in place. The node count does not change."""
        if 0 <= index < len(self._nodes):
            self._nodes[index] = node
            return True
        return False

    def unhealthy(self):
        return [n for n in self._nodes if not n.healthy]
