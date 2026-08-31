#!/usr/bin/env python3
"""Build the solution video: narration audio, slides, and the muxed result.

The narration comes from `tools/tts/narration.txt`, which is extracted from
`docs/video-script.md` so the spoken words cannot drift from the script. This
script splits it into paragraphs, renders each paragraph to its own audio clip,
pairs each with a slide, and concatenates the lot with ffmpeg so every slide is
on screen for exactly as long as its sentence is spoken.

Paragraph-level granularity matters: one slide per section would leave a static
frame up for the best part of a minute. One slide per paragraph gives roughly
one image every ten to twenty seconds, which is close to how fast a person
actually reads a terminal.

    python scripts/build_video.py --check     # slide plan + duration, no render
    python scripts/build_video.py             # full build

Requires: kokoro-onnx + the model files (see tools/tts/README.md), Pillow,
soundfile, and ffmpeg on PATH.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
NARRATION = ROOT / "tools" / "tts" / "narration.txt"
BUILD = ROOT / "tools" / "tts" / "video"
SLIDES = BUILD / "slides"
CLIPS = BUILD / "clips"
OUT = BUILD / "verified-code-reviewer.mp4"

W, H = 1920, 1080
SAMPLE_RATE = 24000
VOICE = "af_heart"

BG = (13, 17, 23)
FG = (201, 209, 217)
DIM = (110, 118, 129)
ACCENT = (88, 166, 255)
GOOD = (63, 185, 80)
BAD = (248, 81, 73)
RULE = (33, 38, 45)


# --------------------------------------------------------------------------
# Slide content. One entry per narration paragraph, in order.
#
# `body` is shown in monospace. Lines beginning with a marker are coloured:
#   "+ "  green   "- "  red   ">>"  accent (highlight)   "//" dim (commentary)
# --------------------------------------------------------------------------
SLIDES_SPEC: list[dict] = [
    # --- 1. the problem -----------------------------------------------------
    dict(
        title="The question a reviewer actually has",
        body="""// Not "what looks suspicious" -- "is this actually broken?"

  A Rust pull request, in code you did not write.

  Three passing tests.
  An explicit bounds check.
  A doc comment promising it cannot panic.""",
    ),
    dict(
        title="benchmark/cases/c12-slot-guard-capacity/diff.patch",
        body="""+pub fn fetch(store: &Store, index: usize) -> Option<&Record> {
+    if index >= store.len() {
+        return None;
+    }
+    Some(store.record_at(index))
+}

// Reads as correct. Three tests pass. It is broken.""",
    ),
    dict(
        title="The deciding fact is not in the diff",
        body="""  src/api.rs   (changed)      if index >= store.len()

  src/store.rs (NOT changed)  pub fn len(&self) -> usize {
>>                                self.capacity      // not the count
                              }

// capacity 100, holding 3 -> index 50 passes the guard and panics.""",
    ),
    # --- 2. the baseline ----------------------------------------------------
    dict(
        title="The simple baseline: one direct review pass",
        body="""  Same model.  Same output schema.
  Same view of the diff and every changed file.

- Withheld: repository tools.""",
    ),
    dict(
        title="What the baseline reports on c12",
        body="""  $ vcr run --agent baseline

  findings: 0

// Not reasoning badly. Reasoning correctly from
// insufficient information.""",
    ),
    dict(
        title="Baseline, 12 cases x 15 trials",
        body="""  precision   1.000
  recall      0.750
  F1          0.857     identical in all 15 trials

- Misses both cases whose evidence sits outside the diff.""",
    ),
    # --- 3. one execution ---------------------------------------------------
    dict(
        title="Four roles, each a separate stateless request",
        body="""  1. Reviewer       propose candidates
  2. Falsifier      what would prove this WRONG?
  3. Investigator   search / read, bounded, sandboxed
  4. Fresh verifier claim + evidence, nothing else

// Rust orchestrates. No role can promote its own finding.""",
    ),
    dict(
        title="1. The candidate",
        body="""  fetch assumes every index below store.len() is a
  valid record, which panics if slots can be vacant.

  src/api.rs:8-13     Correctness     Medium""",
    ),
    dict(
        title="2. The falsification question, fixed BEFORE any lookup",
        body=""">> "Does Store guarantee every index below len()
>>  is occupied and valid for record_at?"

// A separate call on purpose.
// A question written after the verdict just rationalises it.""",
    ),
    dict(
        title="3. Investigation -- Rust runs the tools",
        body="""  search  "struct Store"        -> src/store.rs:13
  read    src/store.rs:1-80     -> 80 lines

>> src/store.rs is the file the change never touched.

// The model can request a lookup and read the result.
// It cannot author an evidence item.""",
    ),
    dict(
        title="4. Fresh-context verification",
        body="""  Receives: the claim + the gathered excerpts.
- Never sees: the reviewer's reasoning.
- Never sees: that anything already believed the claim.

  Verdict: Supports
  "Store::len returns self.capacity, while record_at
   indexes self.records directly." """,
    ),
    dict(
        title="5. Rust assigns the status, not the model",
        body="""  Supports  + repository evidence   -> Verified
>> Supports  + no evidence           -> Uncertain
  Contradicts                       -> Rejected

// "The model said so" is the standard this project rejects.""",
    ),
    # --- 4. the comparison --------------------------------------------------
    dict(
        title="12 frozen cases, 15 trials per arm",
        body="""                    baseline    advanced
  F1                 0.857       0.992
  recall             0.750       1.000     every defect, every trial
  precision          1.000       0.985
  cost / file       $0.0032     $0.0157""",
    ),
    dict(
        title="Switch the stages off one at a time",
        body="""  simple baseline                    0.857
- advanced prompt alone              0.742   worse than nothing clever
- + repository investigation         0.828   still below baseline
+ + falsification (the full system)  0.992""",
    ),
    dict(
        title="They are one mechanism, not two improvements",
        body="""  investigation   buys recall        0.750 -> 1.000
  falsification   makes it affordable 0.707 -> 0.985

>> If you can only ship half of it, ship neither.""",
    ),
    # --- 5. the changelog ---------------------------------------------------
    dict(
        title="Every iteration logged -- including five that made it worse",
        body="""  docs/improvement-changelog.md

  v4  "the code's own claims are not evidence"     REMOVED
  A3  seed the claimed region as evidence          REVERTED
  dedup  merge duplicate candidates                WRONG
  ...""",
    ),
    dict(
        title="One experiment we removed",
        body="""  The verifier rejected a real panic because a doc
  comment claimed callers check first. The comment
  was false. So we told it: comments are not evidence.

- It then rejected two REAL defects, whose facts
- were also written in comments.

  F1  0.933 -> 0.857.   Reverted.""",
    ),
    dict(
        title="And one we had wrong in the other direction",
        body="""  We reported three features as contributing nothing.
  Then we replayed one over every run ever recorded:

>> it fires 7 times, and is WRONG every time.

- The test suite was evidence FOR the bug:
- it asserted the bad merge was correct.""",
    ),
    # --- 6. boundary + hot take --------------------------------------------
    dict(
        title="One more result -- the most useful one we have",
        body="""  Everything so far is measured on a benchmark
  we wrote ourselves.

// So we had other people write the cases.
// Or rather: other agents, that could not see us.""",
    ),
    dict(
        title="16 more cases, written by agents that could not see us",
        body="""  no prompts.  no pipeline.  no docs.  no results.

  holdout    baseline 0.750   advanced 0.944   replicates
- holdout2   baseline 1.000   advanced 1.000   no advantage
- holdout3   baseline 1.000   advanced 0.889   we lose""",
    ),
    dict(
        title="Why: those cases are legible in the diff",
        body="""  The changed line is a recognisable smell, so a
  reviewer flags it on sight and the investigation
  is redundant.

>> A defect is not hard because the evidence is in
>> another file. It is hard when the changed line
>> LOOKS CORRECT.

// We did not rewrite those cases afterwards.""",
    ),
    dict(
        title="Hot take",
        body=""">> Falsification filters for truth, not significance --
>> and most of what a reviewer should suppress is true.

  Worst false positive:
  "this function returns Option but never returns None"

  Accurate. Not a bug. Confirmed correctly, because we
  asked "is this true?" -- almost never the question
  that matters.""",
    ),
    dict(
        title="The lesson we would carry",
        body=""">> A verification step inherits whatever question
>> you ask it.

  Ask the wrong one and it will answer perfectly,
  and still hand a human noise.""",
    ),
]


def paragraphs() -> list[str]:
    """Narration split into paragraphs, section headings dropped."""
    text = NARRATION.read_text(encoding="utf-8")
    out = []
    for block in text.split("\n\n"):
        block = block.strip()
        if not block or block.startswith("## "):
            continue
        out.append(" ".join(block.split()))
    return out


def font(size: int, mono: bool = True):
    from PIL import ImageFont

    names = (
        ["consola.ttf", "DejaVuSansMono.ttf", "cour.ttf"]
        if mono
        else ["segoeui.ttf", "DejaVuSans.ttf", "arial.ttf"]
    )
    for n in names:
        for base in ("C:/Windows/Fonts/", "/usr/share/fonts/truetype/dejavu/", ""):
            try:
                return ImageFont.truetype(base + n, size)
            except OSError:
                continue
    return ImageFont.load_default()


def render_slide(spec: dict, index: int, total: int) -> pathlib.Path:
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)

    title_f = font(46, mono=False)
    body_f = font(34)
    foot_f = font(24, mono=False)

    d.text((110, 92), spec["title"], font=title_f, fill=ACCENT)
    d.line([(110, 172), (W - 110, 172)], fill=RULE, width=3)

    # `+` and `-` are real diff markers and stay on screen; `//` reads as a
    # Rust comment and stays too. `>>` is ours -- a highlight instruction --
    # so it is stripped and expressed as colour alone.
    lines = spec["body"].split("\n")
    line_h = 52
    y = max(232, 232 + ((H - 300 - 232) - len(lines) * line_h) // 2)

    for line in lines:
        colour, text = FG, line
        if line.startswith("+"):
            colour = GOOD
        elif line.startswith("- "):
            colour = BAD
        elif line.startswith(">>"):
            colour, text = ACCENT, "  " + line[2:]
        elif line.startswith("//"):
            colour = DIM
        d.text((130, y), text, font=body_f, fill=colour)
        y += line_h

    d.text(
        (110, H - 78),
        "Verified Code Reviewer",
        font=foot_f,
        fill=DIM,
    )
    d.text(
        (W - 260, H - 78),
        f"{index + 1} / {total}",
        font=foot_f,
        fill=DIM,
    )

    SLIDES.mkdir(parents=True, exist_ok=True)
    path = SLIDES / f"{index:02d}.png"
    img.save(path)
    return path


def synth(paras: list[str]) -> list[float]:
    """Render one audio clip per paragraph; return their durations."""
    import numpy as np
    import soundfile as sf
    from kokoro_onnx import Kokoro

    model = ROOT / "tools" / "tts" / "kokoro-v1.0.onnx"
    voices = ROOT / "tools" / "tts" / "voices-v1.0.bin"
    k = Kokoro(str(model), str(voices))

    CLIPS.mkdir(parents=True, exist_ok=True)
    durations = []
    for i, para in enumerate(paras):
        samples, sr = k.create(para, voice=VOICE, speed=1.0, lang="en-us")
        # A short tail so slides do not cut on the final consonant.
        samples = np.concatenate([samples, np.zeros(int(sr * 0.45), dtype=samples.dtype)])
        path = CLIPS / f"{i:02d}.wav"
        sf.write(path, samples, sr)
        durations.append(len(samples) / sr)
        print(f"  clip {i:02d}  {durations[-1]:5.1f}s  {para[:58]}...")
    return durations


def build(paras: list[str], durations: list[float]) -> None:
    concat = BUILD / "concat.txt"
    lines = []
    for i, dur in enumerate(durations):
        png = (SLIDES / f"{i:02d}.png").as_posix()
        lines.append(f"file '{png}'")
        lines.append(f"duration {dur:.3f}")
    # ffmpeg's concat demuxer needs the last image repeated.
    lines.append(f"file '{(SLIDES / f'{len(durations) - 1:02d}.png').as_posix()}'")
    concat.write_text("\n".join(lines), encoding="utf-8")

    audio_list = BUILD / "audio.txt"
    audio_list.write_text(
        "\n".join(f"file '{(CLIPS / f'{i:02d}.wav').as_posix()}'" for i in range(len(durations))),
        encoding="utf-8",
    )
    full_audio = BUILD / "narration.wav"
    subprocess.run(
        ["ffmpeg", "-y", "-f", "concat", "-safe", "0", "-i", str(audio_list),
         "-c", "copy", str(full_audio)],
        check=True, capture_output=True,
    )
    subprocess.run(
        ["ffmpeg", "-y",
         "-f", "concat", "-safe", "0", "-i", str(concat),
         "-i", str(full_audio),
         "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "25",
         "-c:a", "aac", "-b:a", "160k", "-shortest", str(OUT)],
        check=True, capture_output=True,
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="plan only, no rendering")
    args = ap.parse_args()

    paras = paragraphs()
    print(f"narration paragraphs: {len(paras)}")
    print(f"slide specs:          {len(SLIDES_SPEC)}")
    if len(paras) != len(SLIDES_SPEC):
        print("\nMISMATCH -- every narration paragraph needs exactly one slide.")
        for i in range(max(len(paras), len(SLIDES_SPEC))):
            p = paras[i][:64] if i < len(paras) else "(no paragraph)"
            s = SLIDES_SPEC[i]["title"][:44] if i < len(SLIDES_SPEC) else "(no slide)"
            print(f"  {i:02d}  {s:<46} | {p}")
        return 1

    words = sum(len(p.split()) for p in paras)
    print(f"words: {words}  (~{words / 167 * 60:.0f}s at the measured rate)")

    if args.check:
        for i, (p, s) in enumerate(zip(paras, SLIDES_SPEC)):
            print(f"  {i:02d}  {s['title'][:52]:<54} {len(p.split()):>3}w")
        return 0

    if not shutil.which("ffmpeg"):
        print("ffmpeg not found on PATH")
        return 1

    print("\nrendering slides...")
    for i, spec in enumerate(SLIDES_SPEC):
        render_slide(spec, i, len(SLIDES_SPEC))

    print("synthesising narration...")
    durations = synth(paras)
    total = sum(durations)
    print(f"\ntotal narration: {total:.1f}s")
    if total > 300:
        print(f"OVER THE 300s LIMIT by {total - 300:.1f}s -- cut the script and rerun")
        return 1

    print("muxing...")
    build(paras, durations)
    print(f"\nwrote {OUT}  ({total:.1f}s, {len(paras)} slides)")
    print(f"headroom against the 5:00 limit: {300 - total:.0f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
