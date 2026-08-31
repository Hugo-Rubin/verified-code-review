#!/usr/bin/env python3
"""Extract the spoken narration from the video script.

`docs/video-script.md` is the single source of truth for the video: it holds
the shot list, the commands to run, and the words to say. The words to say are
the block-quoted lines. This pulls them out into the plain-text format
`tools/tts/narrate.py` expects, so the narration cannot drift away from the
script it came from.

    python scripts/extract_narration.py
    python scripts/extract_narration.py --check

`--check` exits non-zero if the extracted narration differs from what is on
disk, which is the useful mode once the file has been rendered once.

Numbers are respoken rather than left as digits: a TTS model reads "0.988" and
"F1" unreliably, and re-recording one section because a decimal came out wrong
is a waste of a take.
"""

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "docs" / "video-script.md"
OUT = ROOT / "tools" / "tts" / "narration.txt"

# Spoken forms. Order matters: longer patterns first.
SPOKEN = [
    (r"\bF1\b", "F one"),
    (r"\bVARCHAR\(64\)\b", "varchar sixty-four"),
    (r"\bunwrap\b", "unwrap"),
    (r"\b0\.988\b", "zero point nine eight eight"),
    (r"\b0\.978\b", "zero point nine seven eight"),
    (r"\b0\.941\b", "zero point nine four one"),
    (r"\b0\.933\b", "zero point nine three three"),
    (r"\b0\.917\b", "zero point nine one seven"),
    (r"\b0\.889\b", "zero point eight eight nine"),
    (r"\b0\.857\b", "zero point eight five seven"),
    (r"\b0\.828\b", "zero point eight two eight"),
    (r"\b0\.750\b", "zero point seven five zero"),
    (r"\b0\.742\b", "zero point seven four two"),
    (r"\b0\.725\b", "zero point seven two five"),
    (r"\b0\.667\b", "zero point six six seven"),
    (r"\b1\.000\b", "one point zero"),
    (r"\b0\.75\b", "zero point seven five"),
]


def spoken(text: str) -> str:
    for pattern, replacement in SPOKEN:
        text = re.sub(pattern, replacement, text)
    # Markdown emphasis is invisible to a listener and confuses the phonemiser.
    text = re.sub(r"\*\*(.+?)\*\*", r"\1", text)
    text = re.sub(r"\*(.+?)\*", r"\1", text)
    text = re.sub(r"`(.+?)`", r"\1", text)
    # An em dash renders as a pause; a comma is the reliable way to get one.
    text = text.replace(" — ", ", ").replace("—", ", ")
    text = re.sub(r",\s*,", ",", text)
    return re.sub(r"\s+", " ", text).strip()


def extract() -> str:
    lines = SCRIPT.read_text(encoding="utf-8").splitlines()
    out: list[str] = []
    section: str | None = None
    para: list[str] = []
    emitted_for_section = False

    def flush():
        nonlocal para
        if para:
            out.append(spoken(" ".join(para)))
            out.append("")
            para = []

    for line in lines:
        heading = re.match(r"^##\s+(.*)$", line)
        if heading:
            flush()
            title = heading.group(1).strip()
            # Timing sections only; skip "Before recording", "Recording notes".
            section = title if re.match(r"^\d", title) else None
            emitted_for_section = False
            continue

        if section is None:
            continue

        if line.startswith(">"):
            body = line[1:].strip()
            if not body:
                flush()
                continue
            if not emitted_for_section:
                out.append(f"## {section}")
                out.append("")
                emitted_for_section = True
            para.append(body)
        else:
            flush()

    flush()
    # Collapse runs of blank lines.
    text = "\n".join(out)
    return re.sub(r"\n{3,}", "\n\n", text).strip() + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    narration = extract()
    words = len(narration.split())
    # Measured against this voice rather than assumed: a 47-word paragraph
    # rendered to 16.9 s, i.e. 167 words per minute. The earlier 150 wpm guess
    # understated the budget by about 10%.
    estimate = words / 167 * 60

    if args.check:
        if not OUT.exists():
            print(f"{OUT} does not exist; run without --check first")
            return 1
        if OUT.read_text(encoding="utf-8") != narration:
            print(f"{OUT} is out of date with {SCRIPT}")
            return 1
        print(f"{OUT} matches the script ({words} words, ~{estimate:.0f}s)")
        return 0

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(narration, encoding="utf-8", newline="\n")
    sections = narration.count("\n## ") + narration.startswith("## ")
    print(f"wrote {OUT}")
    print(f"  {sections} section(s), {words} words, ~{estimate:.0f}s at 167 wpm (measured)")
    if estimate > 300:
        print("  WARNING: over the 300-second video limit; cut before recording")
    else:
        print(f"  headroom against the 300-second limit: ~{300 - estimate:.0f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
