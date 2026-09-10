#!/usr/bin/env python3
"""openWakeWord sidecar for Prosopon.

Reads 16 kHz mono int16 PCM from stdin, runs openWakeWord, and prints a
single line ``WAKE`` to stdout (flushed) each time the wake word is detected
above the threshold.

Protocol (stdin/stdout):
  stdin  : raw int16 little-endian PCM, 16 kHz mono, streamed continuously.
  stdout : the literal line ``WAKE`` on each detection.

Usage:
  python3 wake_word.py --model hey_jarvis --threshold 0.5

The ``--model`` argument is either a bundled openWakeWord model name
(e.g. ``hey_jarvis``, ``alexa``) or a path to a custom ``.tflite`` model.
"""

import argparse
import sys

import numpy as np


def main() -> None:
    parser = argparse.ArgumentParser(description="openWakeWord sidecar")
    parser.add_argument(
        "--model",
        default="hey_jarvis",
        help="bundled openWakeWord model name or path to a custom .tflite",
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.5,
        help="detection threshold (0.0-1.0)",
    )
    parser.add_argument(
        "--chunk",
        type=int,
        default=1280,
        help="samples per chunk (1280 = 80 ms at 16 kHz)",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="log the model score for every frame",
    )
    args = parser.parse_args()

    # Import lazily so a missing dependency produces a clear error rather
    # than a crash at import time.
    try:
        from openwakeword.model import Model
    except ImportError as exc:
        sys.stderr.write(
            "openwakeword is not installed. Run: pip install openwakeword\n"
        )
        raise SystemExit(1) from exc

    model = Model(wakeword_models=[args.model], inference_framework="onnx")

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

        audio = np.frombuffer(buf[:bytes_per_chunk], dtype=np.int16)
        prediction = model.predict(audio)
        score = float(prediction[args.model])

        if args.debug:
            sys.stderr.write(f"score={score:.4f}\n")
            sys.stderr.flush()

        if score >= args.threshold:
            sys.stdout.write("WAKE\n")
            sys.stdout.flush()

        buf = buf[bytes_per_chunk:]


if __name__ == "__main__":
    main()
