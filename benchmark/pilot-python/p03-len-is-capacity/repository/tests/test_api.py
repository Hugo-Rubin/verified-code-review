"""Tests for the cache read endpoints. Run with: python -m pytest"""

import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "src"))

from api import fetch, fetch_many  # noqa: E402
from cache import Page, PagedCache  # noqa: E402


def cache_with(n, capacity):
    c = PagedCache(capacity)
    for i in range(n):
        c.add(Page(i, f"body-{i}"))
    return c


def test_len_reports_configured_capacity():
    c = PagedCache(10)
    assert len(c) == 10
    assert c.filled == 0


def test_add_fills_slots_until_capacity():
    c = PagedCache(2)
    assert c.add(Page(0, "a"))
    assert c.add(Page(1, "b"))
    assert not c.add(Page(2, "c"))
    assert c.filled == 2


def test_fetches_a_present_page():
    c = cache_with(3, 3)
    assert fetch(c, 1).number == 1


def test_returns_none_past_the_end():
    c = cache_with(3, 3)
    assert fetch(c, 3) is None
    assert fetch(c, 99) is None


def test_fetch_many_skips_out_of_range():
    c = cache_with(2, 2)
    assert len(fetch_many(c, [0, 5, 1])) == 2
