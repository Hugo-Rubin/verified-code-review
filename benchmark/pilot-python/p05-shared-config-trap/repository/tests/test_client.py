"""Tests for client construction. Run with: python -m pytest"""

import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "src"))

from client import build_client, default_retry_budget  # noqa: E402
from defaults import BASE, base_settings, with_overrides  # noqa: E402


def test_base_settings_returns_a_copy():
    a = base_settings()
    a["retries"] = 99
    assert BASE["retries"] == 3


def test_overrides_do_not_touch_the_shared_defaults():
    merged = with_overrides({"timeout_secs": 5})
    assert merged["timeout_secs"] == 5
    assert BASE["timeout_secs"] == 30


def test_build_client_applies_overrides():
    c = build_client({"retries": 7})
    assert "retries=7" in c.describe()


def test_building_a_client_leaves_the_defaults_alone():
    build_client({"retries": 7})
    assert default_retry_budget() == 3
    assert "user_agent" not in BASE
