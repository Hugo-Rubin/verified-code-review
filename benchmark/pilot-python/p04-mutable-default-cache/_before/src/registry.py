"""Feature-flag registry."""


class Flag:
    def __init__(self, name, enabled=False):
        self.name = name
        self.enabled = enabled


def collect_enabled(flags, into=None):
    """Append the names of every enabled flag to ``into`` and return it.

    ``into`` lets a caller accumulate across several flag sets.
    """
    if into is None:
        into = []
    for flag in flags:
        if flag.enabled:
            into.append(flag.name)
    return into


def summarise(flags):
    """Names of the enabled flags, as a fresh list each call."""
    return collect_enabled(flags)
