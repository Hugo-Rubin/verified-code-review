"""Tests for the flag registry. Run with: python -m pytest"""

import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "src"))

from registry import Flag, collect_enabled, summarise  # noqa: E402


def test_collects_enabled_flags():
    flags = [Flag("a", True), Flag("b", False), Flag("c", True)]
    assert summarise(flags) == ["a", "c"]


def test_accumulates_into_a_supplied_list():
    acc = []
    collect_enabled([Flag("a", True)], acc)
    collect_enabled([Flag("b", True)], acc)
    assert acc == ["a", "b"]


def test_ignores_disabled_flags():
    assert collect_enabled([Flag("x", False)], []) == []
