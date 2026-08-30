"""Tests for the uploader. Run with: python -m pytest"""

import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "src"))

from manifest import build_manifest, is_complete  # noqa: E402
from uploader import Uploader  # noqa: E402


class FlakyTransport:
    """Fails the first ``fail_times`` calls for each chunk, then succeeds."""

    def __init__(self, fail_times=0):
        self.fail_times = fail_times
        self.seen = {}

    def put(self, chunk_id, payload):
        self.seen[chunk_id] = self.seen.get(chunk_id, 0) + 1
        if self.seen[chunk_id] <= self.fail_times:
            raise ConnectionError("transient")
        return f"etag-{chunk_id}"


def test_uploads_a_chunk():
    u = Uploader(FlakyTransport())
    receipt = u.upload_chunk(0, b"data")
    assert receipt.etag == "etag-0"
    assert u.attempts_made == 1


def test_retries_a_transient_failure():
    u = Uploader(FlakyTransport(fail_times=2), max_attempts=3)
    receipt = u.upload_chunk(0, b"data")
    assert receipt.etag == "etag-0"
    assert u.attempts_made == 3


def test_uploads_every_chunk():
    u = Uploader(FlakyTransport())
    receipts = u.upload_all([b"a", b"b", b"c"])
    manifest = build_manifest(receipts)
    assert manifest["chunks"] == 3
    assert is_complete(manifest, 3)
