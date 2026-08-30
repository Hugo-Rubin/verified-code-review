"""Immutable default settings.

`BASE` is module-level and shared. It is never handed out directly: every
accessor below returns a copy, and nothing in this package mutates it.
"""

BASE = {
    "retries": 3,
    "timeout_secs": 30,
    "verify_tls": True,
}


def base_settings():
    """A fresh copy of the defaults. Callers may mutate the result freely."""
    return dict(BASE)


def with_overrides(overrides):
    """The defaults, with ``overrides`` applied, as a new dict."""
    merged = base_settings()
    merged.update(overrides)
    return merged
