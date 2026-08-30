"""Manifest assembly for a completed upload."""


def build_manifest(receipts):
    """Build the manifest a client uses to reassemble an upload.

    Every receipt contributes one entry, in chunk order.
    """
    return {
        "chunks": len(receipts),
        "etags": [r.etag for r in receipts],
    }


def is_complete(manifest, expected_chunks):
    """True when the manifest covers every expected chunk."""
    return manifest["chunks"] == expected_chunks
