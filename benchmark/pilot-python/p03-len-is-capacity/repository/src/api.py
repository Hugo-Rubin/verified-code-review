"""Read endpoints over a paged cache."""


def fetch(cache, index):
    """Return the page in slot ``index``, or None when out of range.

    Bounds-checked, so an out-of-range index is never an error.
    """
    if index < 0 or index >= len(cache):
        return None
    return cache.page_at(index)


def fetch_many(cache, indices):
    """Fetch several slots, skipping any that are out of range."""
    found = [fetch(cache, i) for i in indices]
    return [p for p in found if p is not None]
