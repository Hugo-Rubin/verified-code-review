"""Cluster status reporting."""


def status_report(cluster):
    """Summarise a cluster's health.

    The first node is the primary by convention.
    """
    nodes = cluster.nodes()

    return {
        "total": len(nodes),
        "primary": nodes[0].name,
        "unhealthy": len(cluster.unhealthy()),
    }


def all_healthy(report):
    return report["unhealthy"] == 0
