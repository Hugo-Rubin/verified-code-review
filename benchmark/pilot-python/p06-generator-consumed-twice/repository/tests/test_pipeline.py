"""Tests for the ingestion pipeline. Run with: python -m pytest"""

import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "src"))

from pipeline import parse_records, summarise  # noqa: E402

LINES = ["a=1", "", "b=2", "c=3"]


def test_parses_key_value_lines():
    records = list(parse_records(LINES))
    assert [r.key for r in records] == ["a", "b", "c"]


def test_skips_blank_and_malformed_lines():
    records = list(parse_records(["a=1", "   ", "nonsense"]))
    assert len(records) == 1


def test_summarise_counts_a_list():
    summary = summarise(list(parse_records(LINES)))
    assert summary["count"] == 3
    assert summary["keys"] == ["a", "b", "c"]
