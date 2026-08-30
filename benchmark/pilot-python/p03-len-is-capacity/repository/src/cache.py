"""Paged cache backed by a fixed number of slots."""


class Page:
    def __init__(self, number, body):
        self.number = number
        self.body = body


class PagedCache:
    """A cache sized in pages.

    Slots are reserved up front and filled over time, so a cache configured
    for 100 pages holding 3 of them has 97 free slots.
    """

    def __init__(self, capacity):
        self.capacity = capacity
        self._pages = []

    def __len__(self):
        """The number of slots this cache was configured with.

        This is the configured capacity, not the number of pages present.
        Use ``filled`` for that.
        """
        return self.capacity

    @property
    def filled(self):
        """How many slots currently hold a page."""
        return len(self._pages)

    def add(self, page):
        """Store a page. Returns False when every slot is taken."""
        if len(self._pages) >= self.capacity:
            return False
        self._pages.append(page)
        return True

    def page_at(self, index):
        """The page in slot ``index``."""
        return self._pages[index]
