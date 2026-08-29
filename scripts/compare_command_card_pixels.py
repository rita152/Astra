#!/usr/bin/env python3
"""Compare equally sized ChatGPT and GPUI Shell-card crops pixel by pixel."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance


def parse_box(value: str) -> tuple[int, int, int, int]:
    values = tuple(int(part) for part in value.split(","))
    if len(values) != 4 or values[2] <= 0 or values[3] <= 0:
        raise argparse.ArgumentTypeError("box must be x,y,width,height")
    x, y, width, height = values
    return x, y, x + width, y + height


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--reference-box", required=True, type=parse_box)
    parser.add_argument("--candidate-box", required=True, type=parse_box)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--threshold", type=float, default=0.995)
    args = parser.parse_args()

    reference = Image.open(args.reference).convert("RGB").crop(args.reference_box)
    candidate = Image.open(args.candidate).convert("RGB").crop(args.candidate_box)
    if reference.size != candidate.size:
        raise SystemExit(
            f"crop sizes differ: reference={reference.size}, candidate={candidate.size}"
        )

    reference_bytes = reference.tobytes()
    candidate_bytes = candidate.tobytes()
    absolute_error = [abs(a - b) for a, b in zip(reference_bytes, candidate_bytes)]
    mean_absolute_error = sum(absolute_error) / len(absolute_error)
    similarity = 1.0 - mean_absolute_error / 255.0
    exact_channel_ratio = sum(error == 0 for error in absolute_error) / len(absolute_error)

    report = {
        "reference": str(args.reference),
        "candidate": str(args.candidate),
        "reference_box": args.reference_box,
        "candidate_box": args.candidate_box,
        "crop_size": reference.size,
        "mean_absolute_error": mean_absolute_error,
        "pixel_similarity": similarity,
        "pixel_similarity_percent": similarity * 100.0,
        "exact_channel_ratio": exact_channel_ratio,
        "threshold": args.threshold,
        "passed": similarity >= args.threshold,
    }

    if args.output_dir:
        args.output_dir.mkdir(parents=True, exist_ok=True)
        reference.save(args.output_dir / "chatgpt-shell-card.png")
        candidate.save(args.output_dir / "gpui-shell-card.png")
        difference = ImageChops.difference(reference, candidate)
        difference.save(args.output_dir / "difference.png")
        ImageEnhance.Brightness(difference).enhance(4.0).save(
            args.output_dir / "difference-4x.png"
        )
        (args.output_dir / "similarity.json").write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    print(json.dumps(report, ensure_ascii=False, indent=2))
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
