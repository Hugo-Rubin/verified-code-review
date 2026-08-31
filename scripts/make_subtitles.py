#!/usr/bin/env python3
"""Write an .srt sidecar for the solution video.

The video build times one audio clip per narration paragraph, so paragraph
boundaries are known exactly. Paragraphs run up to 26 seconds, which is far too
long for a single caption, so each is split into sentences and given a share of
its own clip's duration by character count.

That is an approximation *within* a paragraph, but a well-behaved one: it never
accumulates, because every paragraph re-anchors on a measured boundary. A cue
may sit a beat early or late; it cannot drift out across the video.

Deliberately a sidecar rather than burned in. The slides are already text-dense
and a caption bar would land on top of the panel and the progress bar.

    python scripts/make_subtitles.py
"""

from __future__ import annotations

import pathlib
import re
import sys

import soundfile as sf

ROOT = pathlib.Path(__file__).resolve().parents[1]
NARRATION = ROOT / "tools" / "tts" / "narration-paste.txt"
CLIPS = ROOT / "tools" / "tts" / "video" / "clips"
OUT = ROOT / "tools" / "tts" / "video" / "verified-code-reviewer.srt"

MAX_CHARS_PER_LINE = 42
MAX_LINES = 2
# A cue shorter than this is hard to read; longer than this is a wall of text.
MIN_CUE = 1.2
MAX_CUE = 6.0


def sentences(paragraph: str) -> list[str]:
    """Split on sentence ends, keeping decimals and abbreviations intact."""
    parts = re.split(r"(?<=[.?!])\s+(?=[A-Z\"'])", paragraph.strip())
    out: list[str] = []
    for part in parts:
        part = part.strip()
        if not part:
            continue
        # Merge a fragment too short to stand alone into its predecessor.
        if out and len(part) < 25:
            out[-1] = out[-1] + " " + part
        else:
            out.append(part)
    return out or [paragraph.strip()]


def split_long(text: str, seconds: float) -> list[str]:
    """Break a sentence that would exceed MAX_CUE into readable pieces."""
    if seconds <= MAX_CUE:
        return [text]
    pieces = max(2, int(seconds / MAX_CUE) + 1)
    words = text.split()
    per = max(1, len(words) // pieces)
    out, i = [], 0
    while i < len(words):
        out.append(" ".join(words[i:i + per]))
        i += per
    return out


def wrap(text: str) -> str:
    words, lines, cur = text.split(), [], ""
    for w in words:
        if cur and len(cur) + 1 + len(w) > MAX_CHARS_PER_LINE:
            lines.append(cur)
            cur = w
        else:
            cur = f"{cur} {w}".strip()
    if cur:
        lines.append(cur)
    # Never exceed MAX_LINES. The earlier rebalancing pass could produce lines
    # of 68 characters — well past what fits — because it split by character
    # count and ignored the width it was supposed to respect. Overflow now
    # folds into the last line, which stays short because split_long already
    # capped the cue.
    if len(lines) > MAX_LINES:
        lines = lines[: MAX_LINES - 1] + [" ".join(lines[MAX_LINES - 1:])]
    return "\n".join(l for l in lines if l)


def stamp(t: float) -> str:
    ms = int(round(t * 1000))
    h, ms = divmod(ms, 3_600_000)
    m, ms = divmod(ms, 60_000)
    s, ms = divmod(ms, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def main() -> int:
    paras = [p.strip() for p in NARRATION.read_text(encoding="utf-8").split("\n\n") if p.strip()]
    clips = sorted(CLIPS.glob("*.wav"))
    if len(clips) != len(paras):
        print(f"{len(clips)} clips vs {len(paras)} paragraphs — build the video first")
        return 1

    cues: list[tuple[float, float, str]] = []
    t = 0.0
    for para, clip in zip(paras, clips):
        dur = sf.info(clip).duration
        sents = sentences(para)
        total = sum(len(s) for s in sents)
        start = t
        for sent in sents:
            span = dur * len(sent) / total
            for piece in split_long(sent, span):
                share = span * len(piece) / len(sent)
                cues.append((start, start + share, piece))
                start += share
        t += dur

    # Merge anything too brief to read into its neighbour, rather than
    # stretching it over speech that has already moved on.
    merged: list[list] = []
    for a, b, text in cues:
        if merged and (b - a) < MIN_CUE and len(merged[-1][2]) + len(text) < 84:
            merged[-1][1] = b
            merged[-1][2] = merged[-1][2] + " " + text
        else:
            merged.append([a, b, text])
    for i, cue in enumerate(merged):
        if cue[1] - cue[0] < MIN_CUE and i + 1 < len(merged):
            cue[1] = min(cue[0] + MIN_CUE, merged[i + 1][0])

    cues = [(a, b, t) for a, b, t in merged]
    lines_out = []
    for i, (a, b, text) in enumerate(cues, start=1):
        lines_out.append(f"{i}\n{stamp(a)} --> {stamp(b)}\n{wrap(text)}\n")

    OUT.write_text("\n".join(lines_out), encoding="utf-8")
    span = cues[-1][1]
    print(f"wrote {OUT}")
    print(f"  {len(cues)} cues over {span:.1f}s")
    print(f"  longest cue {max(b - a for a, b, _ in cues):.1f}s, "
          f"shortest {min(b - a for a, b, _ in cues):.1f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
