"""Record ingestion pipeline."""


class Record:
    def __init__(self, key, value):
        self.key = key
        self.value = value


def parse_records(lines):
    """Parse ``lines`` into records, skipping blanks.

    Returns a generator so a large input is not held in memory at once.
    """
    return (
        Record(*line.split("=", 1))
        for line in lines
        if line.strip() and "=" in line
    )


def summarise(records):
    """Count the records and list their keys."""
    return {
        "count": sum(1 for _ in records),
        "keys": [r.key for r in records],
    }
