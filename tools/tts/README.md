# Local narration for the solution video

The video narration is synthesised **on this machine**. No API, no account, no
audio leaves the box — which matters here because the narration quotes the
benchmark, the results and parts of the trajectories.

Model: [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M) v1.0, 82M
parameters, **Apache-2.0**, run through `onnxruntime` on CPU. It renders at
roughly 3–4× realtime on this hardware, so a five-minute narration takes well
under two minutes to produce and can be re-cut freely.

## Setup

```bash
pip install kokoro-onnx onnxruntime soundfile numpy
```

The weights are **not** in the repository — together they are ~350 MB and are
freely re-downloadable, so they are gitignored. Fetch them once into this
directory:

```bash
curl -L -o tools/tts/kokoro-v1.0.onnx https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx
```

```bash
curl -L -o tools/tts/voices-v1.0.bin https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin
```

Expected afterwards:

```text
tools/tts/kokoro-v1.0.onnx   ~310 MB
tools/tts/voices-v1.0.bin    ~27 MB
```

## Use

Preview a single line:

```bash
python tools/tts/narrate.py --text "one line to preview" --voice af_heart
```

List the available voices:

```bash
python tools/tts/narrate.py --list-voices
```

Render the full narration:

```bash
python tools/tts/narrate.py --script tools/tts/narration.txt --out tools/tts/audio
```

The script file is plain text. A line starting with `##` opens a new section
and becomes its own `.wav`, so any segment can be re-recorded without
regenerating the rest — useful when one number changes late. Blank lines become
short pauses.

`narrate.py` prints the duration of each section and the total, and warns when
the total exceeds **300 s**, which is the video limit the submission has to
meet. Check that line before cutting the video, not after.

## Why a local model

Three reasons, in order of how much they mattered:

1. **The narration contains results.** Sending it to a hosted TTS service means
   publishing the findings to a third party before submission, for no benefit.
2. **Re-cuts are free.** Numbers changed repeatedly while this project was
   being measured, and per-section rendering means a changed figure costs one
   sentence of re-rendering rather than a whole take.
3. **Licensing is unambiguous.** Apache-2.0 weights, run locally, with no
   terms-of-service question about whether synthesised audio may be used in a
   submitted video.

`tools/tts/audio/` is gitignored: it is generated output, and the `.wav` files
are large.
