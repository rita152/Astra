#!/usr/bin/env python3
"""Compare matched ChatGPT/GPUI image-generation component crops."""

from __future__ import annotations

import argparse
import io
import json
import math
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageCms, ImageEnhance


def crop_argument(value: str) -> tuple[float, float, float, float]:
    try:
        parts = tuple(float(part) for part in value.split(","))
    except ValueError as error:
        raise argparse.ArgumentTypeError("crop must be x,y,width,height") from error
    if len(parts) != 4 or parts[2] <= 0 or parts[3] <= 0:
        raise argparse.ArgumentTypeError("crop must be x,y,width,height with positive size")
    return parts


def physical_crop(
    crop: tuple[float, float, float, float], dpr: float
) -> tuple[int, int, int, int]:
    x, y, width, height = crop
    left = round(x * dpr)
    top = round(y * dpr)
    return left, top, left + round(width * dpr), top + round(height * dpr)


def srgb_rgba(image: Image.Image) -> Image.Image:
    rgba = image.convert("RGBA")
    if profile := image.info.get("icc_profile"):
        source = ImageCms.ImageCmsProfile(io.BytesIO(profile))
        target = ImageCms.createProfile("sRGB")
        rgba = ImageCms.profileToProfile(
            rgba, source, target, outputMode="RGBA", renderingIntent=0
        )
    return rgba


def opaque_rgb(image: Image.Image) -> Image.Image:
    rgba = srgb_rgba(image)
    background = Image.new("RGBA", rgba.size, "white")
    background.alpha_composite(rgba)
    return background.convert("RGB")


def global_ssim(reference: np.ndarray, actual: np.ndarray) -> float:
    reference = reference.astype(np.float64)
    actual = actual.astype(np.float64)
    values: list[float] = []
    c1 = (0.01 * 255.0) ** 2
    c2 = (0.03 * 255.0) ** 2
    for channel in range(3):
        left = reference[:, :, channel]
        right = actual[:, :, channel]
        left_mean = float(left.mean())
        right_mean = float(right.mean())
        left_variance = float(left.var())
        right_variance = float(right.var())
        covariance = float(((left - left_mean) * (right - right_mean)).mean())
        numerator = (2 * left_mean * right_mean + c1) * (2 * covariance + c2)
        denominator = (
            (left_mean**2 + right_mean**2 + c1)
            * (left_variance + right_variance + c2)
        )
        values.append(numerator / denominator if denominator else 1.0)
    return float(np.mean(values))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--reference-crop", required=True, type=crop_argument)
    parser.add_argument("--actual-crop", required=True, type=crop_argument)
    parser.add_argument("--dpr", required=True, type=float)
    parser.add_argument("--threshold", type=float, default=0.995)
    parser.add_argument("--output-json", type=Path)
    parser.add_argument("--diff", type=Path)
    args = parser.parse_args()
    if args.dpr <= 0:
        parser.error("--dpr must be positive")

    reference_box = physical_crop(args.reference_crop, args.dpr)
    actual_box = physical_crop(args.actual_crop, args.dpr)
    with Image.open(args.reference) as reference_source:
        reference = opaque_rgb(reference_source).crop(reference_box)
    with Image.open(args.actual) as actual_source:
        actual = opaque_rgb(actual_source).crop(actual_box)
    if reference.size != actual.size:
        raise SystemExit(
            f"matched CSS crops produced different physical sizes: "
            f"reference={reference.size}, actual={actual.size}"
        )

    reference_array = np.asarray(reference, dtype=np.int16)
    actual_array = np.asarray(actual, dtype=np.int16)
    difference = np.abs(reference_array - actual_array)
    mae = float(difference.mean())
    mse = float(np.square(difference.astype(np.float64)).mean())
    similarity = 1.0 - mae / 255.0
    exact_pixel_ratio = float(np.all(difference == 0, axis=2).mean())
    within = {
        str(tolerance): float(np.all(difference <= tolerance, axis=2).mean())
        for tolerance in (1, 2, 4)
    }
    metrics = {
        "reference": str(args.reference.resolve()),
        "actual": str(args.actual.resolve()),
        "dpr": args.dpr,
        "reference_crop_css": args.reference_crop,
        "actual_crop_css": args.actual_crop,
        "reference_crop_physical": reference_box,
        "actual_crop_physical": actual_box,
        "compared_physical_size": reference.size,
        "algorithm": "ICC-normalize to sRGB, composite transparency over white, then similarity = 1 - mean absolute channel error / 255",
        "normalized_mae_similarity": similarity,
        "threshold": args.threshold,
        "passed": similarity >= args.threshold,
        "mean_absolute_channel_error": mae,
        "root_mean_square_channel_error": math.sqrt(mse),
        "psnr_db": None if mse == 0 else 20.0 * math.log10(255.0 / math.sqrt(mse)),
        "global_ssim": global_ssim(reference_array, actual_array),
        "exact_pixel_ratio": exact_pixel_ratio,
        "pixel_ratio_with_max_channel_error": within,
        "maximum_channel_error": int(difference.max()),
    }
    rendered = json.dumps(metrics, ensure_ascii=False, indent=2)
    print(rendered)
    if args.output_json:
        args.output_json.parent.mkdir(parents=True, exist_ok=True)
        args.output_json.write_text(rendered + "\n", encoding="utf-8")
    if args.diff:
        args.diff.parent.mkdir(parents=True, exist_ok=True)
        visual = ImageEnhance.Contrast(ImageChops.difference(reference, actual)).enhance(4.0)
        visual.save(args.diff)
    return 0 if metrics["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
