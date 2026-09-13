#!/usr/bin/env python3
"""Moonshine Voice STT sidecar for Prosopon.

Reads 16 kHz mono int16 PCM from stdin, runs Moonshine Voice streaming
transcription, and prints transcript lines to stdout.

Protocol (stdin/stdout):
  stdin  : raw int16 little-endian PCM, 16 kHz mono, streamed continuously.
  stdout : one line per transcript event:
             PARTIAL <text>   (line updated, still in progress)
             FINAL <text>     (line completed)
  stderr : human-readable diagnostics (model load, errors).

Usage:
  python3 stt.py --language en --model-arch small-streaming

The ``--model-arch`` argument is one of the Moonshine streaming archs
available for English: tiny-streaming, small-streaming, medium-streaming.
"""

import argparse
import sys

import numpy as np


def main() -> None:
    parser = argparse.ArgumentParser(description="Moonshine Voice STT sidecar")
    parser.add_argument("--language", default="en")
    parser.add_argument(
        "--model-arch",
        default="small-streaming",
        help="tiny-streaming | small-streaming | medium-streaming",
    )
    parser.add_argument(
        "--chunk",
        type=int,
        default=1280,
        help="samples per chunk (1280 = 80 ms at 16 kHz)",
    )
    parser.add_argument(
        "--update-interval",
        type=float,
        default=0.5,
        help="seconds between transcription updates",
    )
    args = parser.parse_args()

    try:
        from moonshine_voice import (
            Transcriber,
            string_to_model_arch,
            get_model_for_language,
            LineCompleted,
            LineUpdated,
            LineTextChanged,
        )
    except ImportError as exc:
        sys.stderr.write(
            "moonshine-voice is not installed. Run: pip install moonshine-voice\n"
        )
        raise SystemExit(1) from exc

    model_arch = string_to_model_arch(args.model_arch)

    sys.stderr.write(
        f"loading model: language={args.language} arch={args.model_arch}\n"
    )
    sys.stderr.flush()
    model_path, resolved_arch = get_model_for_language(
        wanted_language=args.language, wanted_model_arch=model_arch
    )
    sys.stderr.write(f"model ready: {model_path}\n")
    sys.stderr.flush()

    transcriber = Transcriber(
        model_path=model_path,
        model_arch=resolved_arch,
        update_interval=args.update_interval,
    )
    stream = transcriber.create_stream(update_interval=args.update_interval)

    def on_event(event) -> None:
        line = event.line
        text = (line.text or "").strip()
        if isinstance(event, LineCompleted):
            if text:
                sys.stdout.write(f"FINAL {text}\n")
                sys.stdout.flush()
        elif isinstance(event, (LineUpdated, LineTextChanged)):
            if text:
                sys.stdout.write(f"PARTIAL {text}\n")
                sys.stdout.flush()

    stream.add_listener(on_event)
    stream.start()

    stdin = sys.stdin.buffer
    bytes_per_chunk = args.chunk * 2  # int16 = 2 bytes per sample

    buf = b""
    while True:
        raw = stdin.read(bytes_per_chunk - len(buf))
        if not raw:
            # EOF — the Rust side closed the pipe.
            break
        buf += raw
        if len(buf) < bytes_per_chunk:
            continue

        audio_i16 = np.frombuffer(buf[:bytes_per_chunk], dtype=np.int16)
        audio_f32 = audio_i16.astype(np.float32) / 32768.0
        stream.add_audio(audio_f32.tolist(), sample_rate=16000)
        buf = buf[bytes_per_chunk:]

    # EOF: flush any remaining audio and emit final lines.
    stream.stop()
    transcriber.close()


if __name__ == "__main__":
    main()
