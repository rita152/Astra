#!/usr/bin/env python3
"""Crop and compare the ChatGPT and GPUI collaboration activity rows."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance


HARD_MINIMUM_PERCENT = 99.5


def parse_box(value: str) -> tuple[int, int, int, int]:
    try:
        x, y, width, height = (int(part) for part in value.split(","))
    except (TypeError, ValueError):
        raise argparse.ArgumentTypeError("box must be x,y,width,height") from None
    if min(x, y) < 0 or min(width, height) <= 0:
        raise argparse.ArgumentTypeError("box coordinates must be non-negative and sized")
    return x, y, x + width, y + height


def parse_viewport(value: str) -> tuple[int, int]:
    try:
        width, height = (int(part) for part in value.lower().split("x"))
    except (TypeError, ValueError):
        raise argparse.ArgumentTypeError("viewport must be widthxheight") from None
    if min(width, height) <= 0:
        raise argparse.ArgumentTypeError("viewport dimensions must be positive")
    return width, height


def crop(image: Image.Image, box: tuple[int, int, int, int], label: str) -> Image.Image:
    if box[2] > image.width or box[3] > image.height:
        raise SystemExit(f"{label} box {box} exceeds image size {image.size}")
    return image.crop(box)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path, help="ChatGPT screenshot at the stated viewport")
    parser.add_argument("candidate", type=Path, help="GPUI screenshot at the stated viewport")
    parser.add_argument("--reference-box", required=True, type=parse_box)
    parser.add_argument("--candidate-box", required=True, type=parse_box)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--viewport-css", type=parse_viewport, default=(1470, 923))
    parser.add_argument("--device-pixel-ratio", type=float, default=2.0)
    parser.add_argument("--theme", default="light")
    parser.add_argument("--content", default="Collab evidence probe开始工作")
    parser.add_argument("--minimum", type=float, default=HARD_MINIMUM_PERCENT)
    args = parser.parse_args()

    if args.minimum < HARD_MINIMUM_PERCENT:
        parser.error(f"--minimum cannot be below the {HARD_MINIMUM_PERCENT}% hard gate")
    if args.minimum > 100:
        parser.error("--minimum cannot exceed 100")
    if args.device_pixel_ratio <= 0:
        parser.error("--device-pixel-ratio must be positive")

    reference_image = Image.open(args.reference).convert("RGB")
    candidate_image = Image.open(args.candidate).convert("RGB")
    expected_full_size = tuple(
        round(dimension * args.device_pixel_ratio) for dimension in args.viewport_css
    )
    if candidate_image.size != expected_full_size:
        raise SystemExit(
            "candidate screenshot must match the declared CSS viewport and DPR: "
            f"expected={expected_full_size}, candidate={candidate_image.size}"
        )
    reference_is_full_viewport = reference_image.size == expected_full_size
    if not reference_is_full_viewport and args.reference_box != (
        0,
        0,
        reference_image.width,
        reference_image.height,
    ):
        raise SystemExit(
            "a component-only reference capture must use its complete image as "
            "--reference-box"
        )
    reference = crop(reference_image, args.reference_box, "reference")
    candidate = crop(candidate_image, args.candidate_box, "candidate")
    if reference.size != candidate.size:
        raise SystemExit(
            f"crop sizes differ: reference={reference.size}, candidate={candidate.size}"
        )

    expected_width = round((args.reference_box[2] - args.reference_box[0]))
    expected_height = round((args.reference_box[3] - args.reference_box[1]))
    if reference.size != (expected_width, expected_height):
        raise SystemExit("reference crop size does not agree with its pixel box")

    difference = ImageChops.difference(reference, candidate)
    channel_deltas = [
        channel
        for pixel in difference.get_flattened_data()
        for channel in pixel[:3]
    ]
    absolute_error = sum(channel_deltas)
    channel_count = len(channel_deltas)
    pixel_count = reference.width * reference.height
    maximum_error = channel_count * 255
    mean_absolute_error = absolute_error / channel_count
    normalized_similarity = 100.0 * (1.0 - absolute_error / maximum_error)
    exact_channels = sum(delta == 0 for delta in channel_deltas)
    exact_pixels = sum(
        max(pixel[:3]) == 0 for pixel in difference.get_flattened_data()
    )

    args.output.mkdir(parents=True, exist_ok=True)
    reference.save(args.output / "chatgpt-collaboration.png")
    candidate.save(args.output / "gpui-collaboration.png")
    difference.save(args.output / "difference.png")
    ImageEnhance.Brightness(difference).enhance(4.0).save(
        args.output / "difference-4x.png"
    )
    Image.blend(reference, candidate, 0.5).save(args.output / "overlay.png")

    physical_size = [reference.width, reference.height]
    css_size = [
        reference.width / args.device_pixel_ratio,
        reference.height / args.device_pixel_ratio,
    ]
    report = {
        "reference": str(args.reference),
        "candidate": str(args.candidate),
        "viewport_css": list(args.viewport_css),
        "device_pixel_ratio": args.device_pixel_ratio,
        "theme": args.theme,
        "content": args.content,
        "comparison_region": {
            "reference_capture": (
                "full_viewport" if reference_is_full_viewport else "cdp_component_clip"
            ),
            "reference_source_size_physical_px": list(reference_image.size),
            "candidate_source_size_physical_px": list(candidate_image.size),
            "reference_box_physical_px": list(args.reference_box),
            "candidate_box_physical_px": list(args.candidate_box),
            "size_physical_px": physical_size,
            "size_css_px": css_size,
        },
        "algorithm": (
            "normalized_similarity_percent = 100 * "
            "(1 - sum(abs(reference_rgb - candidate_rgb)) / "
            "(pixel_count * 3 * 255))"
        ),
        "raw_metrics": {
            "absolute_rgb_error": absolute_error,
            "maximum_rgb_error": maximum_error,
            "mean_absolute_channel_error": mean_absolute_error,
            "pixel_count": pixel_count,
            "channel_count": channel_count,
            "exact_pixel_count": exact_pixels,
            "exact_channel_count": exact_channels,
        },
        "normalized_similarity_percent": normalized_similarity,
        "minimum_similarity_percent": args.minimum,
        "passed": normalized_similarity >= args.minimum,
    }
    (args.output / "report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
