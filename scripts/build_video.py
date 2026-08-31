#!/usr/bin/env python3
"""Build the solution video: narration audio, slides, and the muxed result.

The narration comes from `tools/tts/narration.txt`, which is extracted from
`docs/video-script.md` so the spoken words cannot drift from the script. This
splits it into paragraphs, renders each paragraph to its own audio clip, pairs
each with a slide, and concatenates the lot with ffmpeg so every slide is on
screen for exactly as long as its sentence is spoken.

Two consequences of that design, both learned the hard way:

* **Speed cannot desynchronise the video.** Each slide's duration is measured
  from its own rendered clip *after* synthesis, so faster speech simply makes
  each slide shorter.
* **Speaking rate is voice-dependent.** `am_michael` runs near 145 wpm and
  `af_heart` near 167, so the same script differs by half a minute between
  voices. Always read the total this script prints.

    python scripts/build_video.py --check     # slide plan, no rendering
    python scripts/build_video.py             # full build

Requires: kokoro-onnx + the model files (see tools/tts/README.md), Pillow,
soundfile, and ffmpeg on PATH.
"""

from __future__ import annotations

import argparse
import pathlib
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
VOICE = "am_michael"

BG = (11, 15, 20)
PANEL = (18, 24, 32)
FG = (214, 221, 229)
DIM = (118, 128, 141)
ACCENT = (88, 166, 255)
GOOD = (63, 185, 80)
BAD = (248, 81, 73)
RULE = (30, 38, 48)

MARGIN = 132
PANEL_TOP = 262
PANEL_BOT = H - 128
LINE_H = 56
COL_GAP = 46


# --------------------------------------------------------------------------
# Slide content. One entry per narration paragraph, in order.
#
# Body lines use " | " to separate columns. The renderer measures every column
# across the whole slide and aligns them, so nothing depends on hand-counted
# spaces -- which is what made the earlier tables look ragged.
#
# A cell may carry a leading marker, which sets its colour:
#     "+"  green     "-"  red     ">>"  accent     "//"  dim commentary
# "+" and "//" are kept on screen because they read as real diff and comment
# syntax; ">>" and "- " are ours and are stripped.
# --------------------------------------------------------------------------
SLIDES_SPEC: list[dict] = [
    dict(
        title="The question a reviewer actually has",
        body="""// Not "what looks suspicious" -- "is this actually broken?"

A Rust pull request, in code you did not write.

Three passing tests.
An explicit bounds check.
A doc comment promising it cannot panic.""",
    ),
    dict(
        title="c12-slot-guard-capacity  .  diff.patch",
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
        body="""src/api.rs | (changed) | if index >= store.len()

src/store.rs | (NOT changed) | pub fn len(&self) -> usize {
 |  | >> ....self.capacity   // capacity, not the count
 |  | }

// capacity 100, holding 3 -> index 50 passes the guard, then panics.""",
    ),
    dict(
        title="The simple baseline: one direct review pass",
        body="""Same model. | Same output schema.
Same view of the diff | and every changed file.

- Withheld: repository tools.""",
    ),
    dict(
        title="What the baseline reports on c12",
        body="""$ vcr run --agent baseline

findings: 0

// Not reasoning badly.
// Reasoning correctly from insufficient information.""",
    ),
    dict(
        title="Baseline  .  12 cases x 15 trials",
        body="""precision | 1.000
recall | 0.750
F1 | 0.857 | identical in all 15 trials

- Misses both cases whose evidence sits outside the diff.""",
    ),
    dict(
        title="Four roles, each a separate stateless request",
        body="""1. Reviewer | propose candidates
2. Falsifier | what would prove this WRONG?
3. Investigator | search / read -- bounded, sandboxed
4. Fresh verifier | claim + evidence, nothing else

// Rust orchestrates. No role can promote its own finding.""",
    ),
    dict(
        title="1.  The candidate",
        body="""fetch assumes every index below store.len() is a valid
record, which panics if slots can be vacant.

src/api.rs:8-13 | Correctness | Medium""",
    ),
    dict(
        title="2.  The falsification question, fixed BEFORE any lookup",
        body=""">> "Does Store guarantee every index below len()
>>  is occupied and valid for record_at?"

// A separate call, on purpose.
// A question written after the verdict just rationalises it.""",
    ),
    dict(
        title="3.  Investigation -- Rust runs the tools",
        body="""search | "struct Store" | -> src/store.rs:13
read | src/store.rs:1-80 | -> 80 lines

>> src/store.rs is the file the change never touched.

// The model can request a lookup and read the result.
// It cannot author an evidence item.""",
    ),
    dict(
        title="4.  Fresh-context verification",
        body="""Receives: | the claim + the gathered excerpts
- Never sees: | the reviewer's reasoning
- Never sees: | that anything already believed the claim

>> Verdict: Supports
"Store::len returns self.capacity, while record_at
 indexes self.records directly." """,
    ),
    dict(
        title="5.  Rust assigns the status, not the model",
        body="""Supports | + repository evidence | -> Verified
>> Supports | + no evidence | -> Uncertain
Contradicts |  | -> Rejected

// "The model said so" is the standard this project rejects.""",
    ),
    dict(
        title="12 frozen cases  .  15 trials per arm",
        body=""" | baseline | advanced
F1 | 0.857 | 0.992
recall | 0.750 | 1.000 | every defect, every trial
precision | 1.000 | 0.985
cost / case | $0.0032 | $0.0157""",
    ),
    dict(
        title="Switch the stages off, one at a time",
        body="""simple baseline | 0.857
- advanced prompt alone | 0.742 | worse than nothing clever
- + repository investigation | 0.828 | still below baseline
+ falsification -- the full system | 0.992""",
    ),
    dict(
        title="One mechanism, not two improvements",
        body="""investigation | buys recall | 0.750 -> 1.000
falsification | makes it affordable | 0.707 -> 0.985

>> If you can only ship half of it, ship neither.""",
    ),
    dict(
        title="Every iteration logged -- five made it worse",
        body="""docs/improvement-changelog.md

v4 | "the code's own claims are not evidence" | REMOVED
A3 | seed the claimed region as evidence | REVERTED
dedup | merge duplicate candidates | WRONG""",
    ),
    dict(
        title="One experiment we removed",
        body="""The verifier rejected a real panic because a doc comment
claimed callers check first. The comment was false.
So we told it: comments are not evidence.

- It then rejected two REAL defects, whose facts
- were also written in comments.

>> F1  0.933 -> 0.857.   Reverted.""",
    ),
    dict(
        title="And one we had wrong the other way",
        body="""We reported three features as contributing nothing.
Then we replayed one over every run ever recorded:

>> it fires 7 times, and is WRONG every time.

- The test suite was evidence FOR the bug:
- it asserted the bad merge was correct.""",
    ),
    dict(
        title="One more result -- the most useful one",
        body="""Everything so far is measured on a benchmark we wrote.

// So we had other people write the cases.
// Or rather: other agents, that could not see us.""",
    ),
    dict(
        title="16 more cases, by agents that could not see us",
        body="""// no prompts . no pipeline . no docs . no results

 | baseline | advanced
holdout | 0.750 | 0.944 | replicates
- holdout2 | 1.000 | 1.000 | no advantage
- holdout3 | 1.000 | 0.889 | we lose""",
    ),
    dict(
        title="Why: those cases are legible in the diff",
        body="""The changed line is a recognisable smell, so a reviewer
flags it on sight and the investigation is redundant.

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

Accurate. | Not a bug.

// Confirmed correctly, because we asked "is this true?"
// -- almost never the question that matters.""",
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


def font(size: int, mono: bool = True, bold: bool = False):
    from PIL import ImageFont

    if mono:
        names = ["consola.ttf", "DejaVuSansMono.ttf", "cour.ttf"]
    else:
        names = (["segoeuib.ttf", "DejaVuSans-Bold.ttf"] if bold
                 else ["segoeui.ttf", "DejaVuSans.ttf", "arial.ttf"])
    for n in names:
        for base in ("C:/Windows/Fonts/", "/usr/share/fonts/truetype/dejavu/", ""):
            try:
                return ImageFont.truetype(base + n, size)
            except OSError:
                continue
    return ImageFont.load_default()


def marker_of(cell: str) -> tuple[tuple[int, int, int], str]:
    """Colour and display text for one cell, honouring its leading marker."""
    if cell.startswith(">>"):
        return ACCENT, cell[2:].lstrip()
    if cell.startswith("//"):
        return DIM, cell
    if cell.startswith("+"):
        return GOOD, cell
    if cell.startswith("- "):
        return BAD, cell[2:]
    return FG, cell


def rows_of(body: str) -> list[list[str]]:
    return [[c.strip() for c in line.split("|")] for line in body.split("\n")]


def reveal_steps(body: str) -> list[int]:
    """How many body rows are visible at each step of a slide's build-up.

    A slide that appears all at once is a slideshow; one that fills in as the
    sentence is spoken reads like someone working. Blank lines separate groups,
    so text arrives in paragraphs rather than one twitchy line at a time.
    """
    lines = body.split("\n")
    steps: list[int] = []
    for i, line in enumerate(lines, start=1):
        if line.strip() == "":
            continue
        if i == len(lines) or lines[i].strip() == "":
            steps.append(i)
    if not steps:
        steps = [len(lines)]
    if steps[-1] != len(lines):
        steps.append(len(lines))
    return steps


def render_slide(spec, index, total, visible=None, progress=0.0, step=0):
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)

    title_f = font(46, mono=False, bold=True)
    body_f = font(34)
    foot_f = font(23, mono=False)

    # An accent bar rather than a full-width rule: less furniture, and it
    # anchors the eye at the left margin where the text begins.
    d.rectangle([MARGIN - 28, 98, MARGIN - 20, 152], fill=ACCENT)
    d.text((MARGIN, 96), spec["title"], font=title_f, fill=(238, 243, 248))

    rows = rows_of(spec["body"])
    ncol = max(len(r) for r in rows)

    # Column widths measured across the finished slide, so every column lines
    # up and nothing shifts as rows arrive.
    # Only genuine table rows set column widths. A single-cell row is prose --
    # a `//` comment, say -- and letting its full length widen column 0 pushed
    # every later column off the right edge.
    widths = [0] * ncol
    for r in rows:
        if len(r) < 2:
            continue
        for c, cell in enumerate(r):
            _, text = marker_of(cell)
            widths[c] = max(widths[c], int(d.textlength(text.replace("....", "    "),
                                                        font=body_f)))
    xs, x = [], MARGIN
    for c in range(ncol):
        xs.append(x)
        x += widths[c] + COL_GAP

    block_h = len(rows) * LINE_H
    y0 = PANEL_TOP + max(0, ((PANEL_BOT - PANEL_TOP) - block_h) // 2)

    # Panel width from what is actually drawn, prose rows included, so nothing
    # overflows it and nothing floats outside it.
    extent = MARGIN + 640
    for r in rows:
        if len(r) < 2:
            if r and r[0]:
                _, text = marker_of(r[0])
                extent = max(extent, MARGIN + int(
                    d.textlength(text.replace("....", "    "), font=body_f)))
            continue
        for c, cell in enumerate(r):
            if not cell:
                continue
            _, text = marker_of(cell)
            extent = max(extent, xs[c] + int(
                d.textlength(text.replace("....", "    "), font=body_f)))

    pad = 42
    d.rounded_rectangle([MARGIN - pad, y0 - pad, min(extent + pad, W - 70),
                         y0 + block_h + pad], radius=20, fill=PANEL)

    y = y0
    shown = len(rows) if visible is None else visible
    for r in rows[:shown]:
        for c, cell in enumerate(r):
            if not cell:
                continue
            colour, text = marker_of(cell)
            d.text((xs[c], y), text.replace("....", "    "), font=body_f, fill=colour)
        y += LINE_H

    d.text((MARGIN, H - 72), "Verified Code Reviewer", font=foot_f, fill=DIM)
    d.text((W - MARGIN - 90, H - 72), f"{index + 1} / {total}", font=foot_f, fill=DIM)

    bar_y = H - 16
    d.line([(0, bar_y), (W, bar_y)], fill=RULE, width=6)
    if progress > 0:
        d.line([(0, bar_y), (int(W * min(progress, 1.0)), bar_y)], fill=ACCENT, width=6)

    SLIDES.mkdir(parents=True, exist_ok=True)
    path = SLIDES / f"{index:02d}_{step:02d}.png"
    img.save(path)
    return path


def synth(paras: list[str], speed: float, voice: str) -> list[float]:
    """Render one audio clip per paragraph; return their durations."""
    import numpy as np
    import soundfile as sf
    from kokoro_onnx import Kokoro

    k = Kokoro(str(ROOT / "tools/tts/kokoro-v1.0.onnx"),
               str(ROOT / "tools/tts/voices-v1.0.bin"))

    CLIPS.mkdir(parents=True, exist_ok=True)
    durations = []
    for i, para in enumerate(paras):
        samples, sr = k.create(para, voice=voice, speed=speed, lang="en-us")
        # A short tail so slides do not cut on the final consonant.
        samples = np.concatenate([samples, np.zeros(int(sr * 0.45), dtype=samples.dtype)])
        sf.write(CLIPS / f"{i:02d}.wav", samples, sr)
        durations.append(len(samples) / sr)
        print(f"  clip {i:02d}  {durations[-1]:5.1f}s  {para[:56]}...")
    return durations


def import_audio(path: pathlib.Path, paras: list[str]) -> list[float]:
    """Use narration rendered elsewhere (e.g. ElevenLabs) instead of Kokoro.

    **A directory** of one clip per paragraph, named so they sort in order, is
    exact: each slide is timed from its own clip, exactly as with Kokoro. This
    is the option to use.

    **A single file** of the whole narration can only be divided by estimate.
    Splitting on detected pauses was tried and abandoned: measured against a
    known-good 23-paragraph recording it was out by 7s on average and 37s at
    worst, because a within-paragraph pause is often longer than a paragraph
    break. Dividing by character count does better -- about 2s mean and 5s
    worst on the same recording -- and that is what this does, with a warning,
    because a 5s drift on a 12s slide is visible.
    """
    CLIPS.mkdir(parents=True, exist_ok=True)
    n = len(paras)

    if path.is_dir():
        files = sorted(f for f in path.iterdir()
                       if f.suffix.lower() in (".mp3", ".wav", ".m4a", ".flac"))
        if len(files) != n:
            print(f"{path} holds {len(files)} clips; the script has {n} paragraphs")
            return []
        durations = []
        for i, f in enumerate(files):
            dst = CLIPS / f"{i:02d}.wav"
            subprocess.run(["ffmpeg", "-y", "-i", str(f), "-ar", "24000", "-ac", "1",
                            str(dst)], check=True, capture_output=True)
            d = float(subprocess.run(
                ["ffprobe", "-v", "error", "-show_entries", "format=duration",
                 "-of", "default=nw=1:nk=1", str(dst)],
                capture_output=True, text=True).stdout.strip())
            durations.append(d)
            print(f"  clip {i:02d}  {d:5.1f}s  <- {f.name}")
        return durations

    total = float(subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration",
         "-of", "default=nw=1:nk=1", str(path)],
        capture_output=True, text=True).stdout.strip())
    print(f"  single file, {total:.1f}s — dividing by character count.")
    print("  APPROXIMATE: expect a second or two of drift, and up to ~5s on the")
    print("  longest slides. Export one clip per paragraph into a directory for")
    print("  exact timing.")

    # Character proportion sets where a boundary should fall; the nearest real
    # pause decides where it actually falls. Even a boundary that is a second
    # out is unnoticeable if the slide changes during silence rather than
    # mid-word, and that is what this buys.
    pr = subprocess.run(
        ["ffmpeg", "-i", str(path), "-af", "silencedetect=noise=-35dB:d=0.35",
         "-f", "null", "-"], capture_output=True, text=True)
    st = [float(l.rsplit("silence_start:", 1)[1].strip())
          for l in pr.stderr.splitlines() if "silence_start:" in l]
    en = [float(l.rsplit("silence_end:", 1)[1].split("|")[0].strip())
          for l in pr.stderr.splitlines() if "silence_end:" in l]
    pauses = [(a + b) / 2 for a, b in zip(st, en)]
    print(f"  {len(pauses)} pauses available to snap to")

    weights = [len(p) for p in paras]
    tot = sum(weights)
    bounds, prev, cum, snapped = [], 0.0, 0, 0
    for w in weights[:-1]:
        cum += w
        want = total * cum / tot
        near = [q for q in pauses if abs(q - want) <= 2.5 and q > prev + 1.0]
        if near:
            cut = min(near, key=lambda q: abs(q - want))
            snapped += 1
        else:
            cut = want
        bounds.append((prev, cut))
        prev = cut
    bounds.append((prev, total))
    print(f"  {snapped} of {len(weights) - 1} boundaries landed on a real pause")

    durations = []
    for i, (a, b) in enumerate(bounds):
        dst = CLIPS / f"{i:02d}.wav"
        subprocess.run(["ffmpeg", "-y", "-i", str(path), "-ss", f"{a:.3f}",
                        "-to", f"{b:.3f}", "-ar", "24000", "-ac", "1", str(dst)],
                       check=True, capture_output=True)
        durations.append(b - a)
        print(f"  clip {i:02d}  {b - a:5.1f}s")
    return durations


def build(durations: list[float]) -> None:
    """Write the concat list, giving each reveal step its share of its clip."""
    total_time = sum(durations)
    lines: list[str] = []
    elapsed = 0.0
    last_png = None

    for i, dur in enumerate(durations):
        steps = reveal_steps(SLIDES_SPEC[i]["body"])
        n = len(steps)
        reveal = dur * 0.55
        hold = dur - reveal
        per_step = reveal / max(n - 1, 1) if n > 1 else 0.0

        for k, visible in enumerate(steps):
            png = render_slide(SLIDES_SPEC[i], i, len(SLIDES_SPEC),
                               visible=visible,
                               progress=elapsed / total_time if total_time else 0.0,
                               step=k)
            seg = (per_step if k < n - 1 else hold) if n > 1 else dur
            elapsed += seg
            lines.append(f"file '{png.as_posix()}'")
            lines.append(f"duration {seg:.3f}")
            last_png = png

    lines.append(f"file '{last_png.as_posix()}'")
    (BUILD / "concat.txt").write_text("\n".join(lines), encoding="utf-8")

    audio_list = BUILD / "audio.txt"
    audio_list.write_text(
        "\n".join(f"file '{(CLIPS / f'{i:02d}.wav').as_posix()}'"
                  for i in range(len(durations))),
        encoding="utf-8")
    full_audio = BUILD / "narration.wav"
    subprocess.run(["ffmpeg", "-y", "-f", "concat", "-safe", "0",
                    "-i", str(audio_list), "-c", "copy", str(full_audio)],
                   check=True, capture_output=True)

    # `-t` pins the container to the narration length. The concat demuxer holds
    # the repeated final image for an unspecified time otherwise, which added
    # nine seconds of silent trailing frame and nearly pushed a 290s narration
    # over the 300s limit.
    subprocess.run(["ffmpeg", "-y",
                    "-f", "concat", "-safe", "0", "-i", str(BUILD / "concat.txt"),
                    "-i", str(full_audio),
                    "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "25",
                    "-c:a", "aac", "-b:a", "160k",
                    "-t", f"{total_time:.3f}", "-shortest", str(OUT)],
                   check=True, capture_output=True)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="plan only, no rendering")
    # Changing speed cannot desynchronise the video: every slide's duration is
    # measured from its own rendered clip, after synthesis.
    ap.add_argument("--speed", type=float, default=1.25)
    ap.add_argument("--voice", default=VOICE)
    ap.add_argument("--audio", type=pathlib.Path,
                    help="narration rendered elsewhere: a directory of one clip "
                         "per paragraph, or a single file split on its pauses")
    args = ap.parse_args()

    paras = paragraphs()
    print(f"narration paragraphs: {len(paras)}")
    print(f"slide specs:          {len(SLIDES_SPEC)}")
    if len(paras) != len(SLIDES_SPEC):
        print("\nMISMATCH -- every narration paragraph needs exactly one slide.")
        for i in range(max(len(paras), len(SLIDES_SPEC))):
            p = paras[i][:60] if i < len(paras) else "(no paragraph)"
            t = SLIDES_SPEC[i]["title"][:44] if i < len(SLIDES_SPEC) else "(no slide)"
            print(f"  {i:02d}  {t:<46} | {p}")
        return 1

    words = sum(len(p.split()) for p in paras)
    print(f"words: {words}")

    if args.check:
        for i, (p, s) in enumerate(zip(paras, SLIDES_SPEC)):
            print(f"  {i:02d}  {s['title'][:52]:<54} {len(p.split()):>3}w")
        return 0

    if not shutil.which("ffmpeg"):
        print("ffmpeg not found on PATH")
        return 1

    if args.audio:
        print(f"importing narration from {args.audio}...")
        durations = import_audio(args.audio, paras)
        if not durations:
            return 1
    else:
        print(f"synthesising narration ({args.voice}, speed {args.speed})...")
        durations = synth(paras, args.speed, args.voice)
    total = sum(durations)
    print(f"\ntotal narration: {total:.1f}s")
    if total > 300:
        print(f"OVER THE 300s LIMIT by {total - 300:.1f}s -- cut the script or raise --speed")
        return 1

    print("rendering slides and muxing...")
    build(durations)
    print(f"\nwrote {OUT}  ({total:.1f}s, {len(SLIDES_SPEC)} slides)")
    print(f"headroom against the 5:00 limit: {300 - total:.0f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
