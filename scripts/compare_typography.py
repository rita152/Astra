"""Compare glyph pixels, excluding blank background from the similarity score."""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def compare(reference, actual, dpr):
    ref = np.asarray(Image.open(reference).convert('RGB'), dtype=float)
    act = np.asarray(Image.open(actual).convert('RGB'), dtype=float)
    expected = (620 * dpr, 1000 * dpr, 3)
    if ref.shape != expected or act.shape != expected:
        raise ValueError(f'Expected {expected}; reference={ref.shape}, actual={act.shape}. No resampling permitted.')
    samples = json.loads(Path(__file__).with_name('typography_samples.json').read_text())
    rows = []
    for dark in (False, True):
        background = np.array([24, 24, 24] if dark else [255, 255, 255])
        foreground = np.array([223, 223, 223] if dark else [26, 28, 31])
        for index, sample in enumerate(samples):
            x, y = (524 if dark else 24) * dpr, (40 + index * 36) * dpr
            w, h = 450 * dpr, 36 * dpr
            a, b = ref[y:y+h, x:x+w], act[y:y+h, x:x+w]
            mask = (np.abs(a-background).max(2) > 8) | (np.abs(b-background).max(2) > 8)
            error = np.abs(a-b)[mask].mean()
            ref_ink = np.clip((a-background)/(foreground-background), 0, 1).mean(2).sum()
            act_ink = np.clip((b-background)/(foreground-background), 0, 1).mean(2).sum()
            rows.append(dict(theme='dark' if dark else 'light', index=index, **sample,
                             glyph_mae=float(error), glyph_similarity=1-float(error)/255,
                             ink_ratio=float(act_ink/ref_ink), glyph_pixels=int(mask.sum())))
    return dict(reference=str(reference), actual=str(actual), dpr=dpr,
                alignment='fixed fixture coordinates; no shifts, resampling or background padding in score',
                glyph_mae=float(np.average([r['glyph_mae'] for r in rows], weights=[r['glyph_pixels'] for r in rows])), rows=rows)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('actual', type=Path)
    parser.add_argument('--dpr', type=int, default=1)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = compare(args.reference, args.actual, args.dpr)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2)+'\n')
    print(f"Glyph MAE: {result['glyph_mae']:.4f} / 255")
    for row in result['rows']:
        print(f"{row['theme']:5} {row['index']:2} {row['weight']:3} {row['size']:2} px: MAE={row['glyph_mae']:7.3f} ink={row['ink_ratio']:.4f}")
