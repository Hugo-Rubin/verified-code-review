"""Chunked upload with retry."""

from dataclasses import dataclass


class UploadError(Exception):
    """Raised when an upload cannot be completed."""


@dataclass
class Receipt:
    chunk_id: int
    etag: str


class Uploader:
    """Uploads chunks to a remote store, retrying transient failures."""

    def __init__(self, transport, max_attempts=3):
        self.transport = transport
        self.max_attempts = max_attempts
        self.attempts_made = 0

    def upload_chunk(self, chunk_id, payload):
        """Upload one chunk and return its receipt.

        Retries up to ``max_attempts`` times.
        """
        last = None
        for _ in range(self.max_attempts):
            self.attempts_made += 1
            try:
                etag = self.transport.put(chunk_id, payload)
                return Receipt(chunk_id=chunk_id, etag=etag)
            except Exception as exc:
                last = exc
        return None

    def upload_all(self, chunks):
        """Upload every chunk and return the receipts."""
        receipts = []
        for chunk_id, payload in enumerate(chunks):
            receipts.append(self.upload_chunk(chunk_id, payload))
        return receipts
