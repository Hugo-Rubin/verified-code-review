#!/usr/bin/env python3
"""Render narration audio for the solution video, locally.

Uses Kokoro (82M params, Apache-2.0) through onnxruntime. No API, no account,
no audio leaves the machine — which matters because the narration quotes the
benchmark and the results.

    python tools/tts/narrate.py --list-voices
    python tools/tts/narrate.py --script tools/tts/narration.txt --out tools/tts/audio
    python tools/tts/narrate.py --text "one line to preview" --voice af_heart

The script file is plain text. A line beginning with `##` starts a new section
and becomes its own .wav, so segments can be re-recorded individually without
regenerating the whole narration. Blank lines separate paragraphs and become a
short pause.

Model files (downloaded once, gitignored):
    tools/tts/kokoro-v1.0.onnx   ~310 MB
    tools/tts/voices-v1.0.bin    ~27 MB
"""

import argparse
import pathlib
import re
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
MODEL = HERE / "kokoro-v1.0.onnx"
VOICES = HERE / "voices-v1.0.bin"
SAMPLE_RATE = 24000

# A calm, clear, mid-paced narrator suits a technical walkthrough better than
# anything expressive: the content is numbers and reasoning, and the delivery
# should stay out of its way.
DEFAULT_VOICE = "af_heart"
DEFAULT_SPEED = 1.0


def load():
    # onnxruntime advertises TensorRT whenever the package is built with it,
    # then fails loudly at session creation if the TensorRT DLLs are absent —
    # which they are on a stock CUDA install. Naming the providers we actually
    # want avoids a wall of red output on every run. CPU alone renders this
    # narration faster than realtime anyway.
    import os

    os.environ.setdefault("ORT_LOGGING_LEVEL", "3")
    os.environ.setdefault("ONNX_PROVIDER", "CPUExecutionProvider")

    try:
        from kokoro_onnx import Kokoro
    except ImportError:
        print("kokoro-onnx is not installed. Run: pip install kokoro-onnx soundfile", file=sys.stderr)
        raise SystemExit(2)

    for f in (MODEL, VOICES):
        if not f.is_file():
            print(f"missing model file: {f}\nSee the header of this script.", file=sys.stderr)
            raise SystemExit(2)

    return Kokoro(str(MODEL), str(VOICES))


def slugify(text: str, index: int) -> str:
    s = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return f"{index:02d}-{s[:48] or 'section'}"


def parse_sections(raw: str):
    """Split a script into (title, body) sections on `##` headings."""
    sections, title, buf = [], "intro", []
    for line in raw.splitlines():
        if line.startswith("##"):
            if buf and any(b.strip() for b in buf):
                sections.append((title, "\n".join(buf).strip()))
            title = line.lstrip("#").strip() or "section"
            buf = []
        else:
            buf.append(line)
    if buf and any(b.strip() for b in buf):
        sections.append((title, "\n".join(buf).strip()))
    return sections


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--script", help="plain-text narration file")
    ap.add_argument("--text", help="render a single string instead of a file")
    ap.add_argument("--out", default=str(HERE / "audio"))
    ap.add_argument("--voice", default=DEFAULT_VOICE)
    ap.add_argument("--speed", type=float, default=DEFAULT_SPEED)
    ap.add_argument("--list-voices", action="store_true")
    args = ap.parse_args()

    kokoro = load()

    if args.list_voices:
        names = sorted(kokoro.get_voices())
        print(f"{len(names)} voices available:\n")
        for n in names:
            print(f"  {n}")
        print("\naf_* / am_* are US English female / male; bf_* / bm_* are British.")
        return 0

    import soundfile as sf

    out_dir = pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    if args.text:
        sections = [("preview", args.text)]
    elif args.script:
        raw = pathlib.Path(args.script).read_text(encoding="utf-8")
        sections = parse_sections(raw)
    else:
        print("give --script or --text", file=sys.stderr)
        return 2

    if not sections:
        print("nothing to narrate", file=sys.stderr)
        return 2

    total_audio = 0.0
    started = time.time()

    for i, (title, body) in enumerate(sections, start=1):
        name = slugify(title, i)
        samples, sr = kokoro.create(body, voice=args.voice, speed=args.speed, lang="en-us")
        path = out_dir / f"{name}.wav"
        sf.write(path, samples, sr)
        seconds = len(samples) / sr
        total_audio += seconds
        print(f"  {path.name:<56} {seconds:6.1f}s")

    wall = time.time() - started
    print(f"\n{len(sections)} file(s), {total_audio:.1f}s of audio in {wall:.1f}s wall "
          f"({total_audio / wall:.1f}x realtime)")
    print(f"written to {out_dir}")

    if total_audio > 300:
        print(f"\nWARNING: {total_audio:.0f}s exceeds the 5-minute video limit by "
              f"{total_audio - 300:.0f}s. Trim the script.")
    else:
        print(f"headroom against the 5-minute limit: {300 - total_audio:.0f}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
