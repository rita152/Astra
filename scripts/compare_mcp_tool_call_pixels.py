#!/usr/bin/env python3
"""Compare same-CSS-size ChatGPT and GPUI MCP tool-call regions."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance


def parse_pair(value: str) -> tuple[float, float]:
    parts = tuple(float(part) for part in value.split(","))
    if len(parts) != 2:
        raise argparse.ArgumentTypeError("expected x,y")
    return parts


def crop_at_css_origin(
    image: Image.Image,
    origin: tuple[float, float],
    size: tuple[float, float],
    dpr: float,
) -> Image.Image:
    left = round(origin[0] * dpr)
    top = round(origin[1] * dpr)
    width = round(size[0] * dpr)
    height = round(size[1] * dpr)
    return image.crop((left, top, left + width, top + height)).convert("RGB")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--reference-origin-css", required=True, type=parse_pair)
    parser.add_argument("--candidate-origin-css", required=True, type=parse_pair)
    parser.add_argument("--size-css", required=True, type=parse_pair)
    parser.add_argument("--reference-dpr", type=float, default=2.0)
    parser.add_argument("--candidate-dpr", type=float, default=2.0)
    parser.add_argument("--channel-tolerance", type=int, default=8)
    parser.add_argument("--threshold", type=float, default=0.995)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()

    if min(*args.size_css, args.reference_dpr, args.candidate_dpr) <= 0:
        raise SystemExit("CSS size and DPR values must be positive")
    reference = crop_at_css_origin(
        Image.open(args.reference),
        args.reference_origin_css,
        args.size_css,
        args.reference_dpr,
    )
    candidate = crop_at_css_origin(
        Image.open(args.candidate),
        args.candidate_origin_css,
        args.size_css,
        args.candidate_dpr,
    )
    if reference.size != candidate.size:
        raise SystemExit(
            f"physical crop mismatch: reference={reference.size}, candidate={candidate.size}"
        )

    errors = [
        abs(reference_channel - candidate_channel)
        for reference_channel, candidate_channel in zip(
            reference.tobytes(), candidate.tobytes()
        )
    ]
    channel_count = len(errors)
    pixel_count = reference.width * reference.height
    absolute_error = sum(errors)
    mean_absolute_error = absolute_error / channel_count
    root_mean_square_error = math.sqrt(sum(error * error for error in errors) / channel_count)
    normalized_similarity = 1.0 - absolute_error / (channel_count * 255.0)
    exact_channel_ratio = sum(error == 0 for error in errors) / channel_count
    within_tolerance_ratio = (
        sum(error <= args.channel_tolerance for error in errors) / channel_count
    )
    channel_mae = [
        sum(errors[channel::3]) / pixel_count for channel in range(3)
    ]
    passed = normalized_similarity >= args.threshold

    args.output_dir.mkdir(parents=True, exist_ok=True)
    reference.save(args.output_dir / "chatgpt-mcp-tool-call.png")
    candidate.save(args.output_dir / "gpui-mcp-tool-call.png")
    difference = ImageChops.difference(reference, candidate)
    difference.save(args.output_dir / "difference.png")
    ImageEnhance.Brightness(difference).enhance(4.0).save(
        args.output_dir / "difference-4x.png"
    )
    Image.blend(reference, candidate, 0.5).save(args.output_dir / "overlay.png")

    report = {
        "region": "mcp-tool-call",
        "algorithm": "1 - mean absolute RGB channel error / 255",
        "reference": str(args.reference),
        "candidate": str(args.candidate),
        "reference_origin_css": args.reference_origin_css,
        "candidate_origin_css": args.candidate_origin_css,
        "css_size": args.size_css,
        "reference_dpr": args.reference_dpr,
        "candidate_dpr": args.candidate_dpr,
        "physical_crop_size": reference.size,
        "pixel_count": pixel_count,
        "channel_count": channel_count,
        "absolute_channel_error": absolute_error,
        "mean_absolute_error": mean_absolute_error,
        "root_mean_square_error": root_mean_square_error,
        "max_channel_error": max(errors, default=0),
        "channel_mae_rgb": channel_mae,
        "exact_channel_ratio": exact_channel_ratio,
        "channel_tolerance": args.channel_tolerance,
        "within_tolerance_ratio": within_tolerance_ratio,
        "normalized_similarity": normalized_similarity,
        "normalized_similarity_percent": normalized_similarity * 100.0,
        "threshold": args.threshold,
        "passed": passed,
    }
    (args.output_dir / "similarity.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if not passed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
