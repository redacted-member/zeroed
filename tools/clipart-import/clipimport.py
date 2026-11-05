"""Converts downloaded CC0 / CC BY art into the clipart library's imported
files and writes the Rust table that lists them.

    python clipimport.py <items.json> <downloads folder> <repo root> [only]

items.json lists every item: its source file under the downloads folder,
the conversion settings and where it came from. Reliefs are rendered from
3D models with the meshdepth tool (tools/clipart-import/meshdepth), tiles
come from displacement maps, shapes from SVG files.
"""

import io
import json
import os
import re
import subprocess
import sys

import numpy as np
from PIL import Image
from scipy import fft, ndimage

HERE = os.path.dirname(os.path.abspath(__file__))
MESHDEPTH = os.environ.get("MESHDEPTH", os.path.join(HERE, "meshdepth", "target", "release", "meshdepth"))


def slug(name):
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def poisson_neumann(gx, gy):
    h, w = gx.shape
    div = np.zeros((h, w))
    div[:, 0] += gx[:, 0]
    div[:, 1:] += gx[:, 1:] - gx[:, :-1]
    div[0, :] += gy[0, :]
    div[1:, :] += gy[1:, :] - gy[:-1, :]
    d = fft.dctn(div, type=2, norm="ortho")
    yy, xx = np.meshgrid(np.arange(h), np.arange(w), indexing="ij")
    den = (2 * np.cos(np.pi * xx / w) - 2) + (2 * np.cos(np.pi * yy / h) - 2)
    den[0, 0] = 1
    u = d / den
    u[0, 0] = 0
    return fft.idctn(u, type=2, norm="ortho")


def depth_map(mesh, px, rot, renormal):
    out = mesh + f".{px}.{'_'.join(str(r) for r in rot)}.{int(renormal)}.f32"
    meta = out + ".json"
    if not os.path.exists(meta):
        env = dict(os.environ)
        if renormal:
            env["MESHDEPTH_RENORMAL"] = "1"
        res = subprocess.run([MESHDEPTH, mesh, out, str(px)] + [str(r) for r in rot],
                             capture_output=True, text=True, env=env, check=True)
        open(meta, "w").write(res.stdout)
    m = json.loads(open(meta).read())
    z = np.fromfile(out, dtype="<f4").reshape(m["height"], m["width"])
    return z, m


def bas_relief(z, cell, it):
    """Heights in mm (NaN outside) at size/px mm per pixel, rows from the lowest Y up."""
    crop = it.get("crop")
    if crop:
        h, w = z.shape
        x0, y0, x1, y1 = crop
        z = z[int(y0 * h):int(np.ceil(y1 * h)), int(x0 * w):int(np.ceil(x1 * w))]
    front, back = np.nanmax(z), np.nanmin(z)
    base = front - it.get("keep", 0.8) * (front - back)
    mask = np.isfinite(z) & (z > base)
    # Islands smaller than a share of the largest are dropped.
    lab, n = ndimage.label(mask)
    if n > 1:
        sizes = ndimage.sum(mask, lab, range(1, n + 1))
        keep_ids = [k + 1 for k, s in enumerate(sizes) if s >= sizes.max() * it.get("island", 0.02)]
        mask = np.isin(lab, keep_ids)
    if it.get("fill"):
        mask = ndimage.binary_fill_holes(mask)
    hgt = np.where(mask, np.where(np.isfinite(z), z, base) - base, 0.0)
    ys, xs = np.nonzero(mask)
    m = 3
    y0, y1 = max(ys.min() - m, 0), min(ys.max() + m + 1, hgt.shape[0])
    x0, x1 = max(xs.min() - m, 0), min(xs.max() + m + 1, hgt.shape[1])
    hgt, mask = hgt[y0:y1, x0:x1], mask[y0:y1, x0:x1]
    size = it["size"]
    px_mm = size / max(hgt.shape)
    hmm = hgt / cell * px_mm
    alpha = it.get("alpha", 1.0)
    if alpha > 0:
        gx = np.zeros_like(hmm)
        gy = np.zeros_like(hmm)
        gx[:, :-1] = hmm[:, 1:] - hmm[:, :-1]
        gy[:-1, :] = hmm[1:, :] - hmm[:-1, :]

        def comp(g):
            return np.sign(g) * np.log1p(alpha * np.abs(g)) / alpha

        r = poisson_neumann(comp(gx), comp(gy))
        r -= np.median(r[~mask]) if (~mask).any() else r.min()
    else:
        r = hmm
    r = np.where(mask, np.maximum(r, 0.0), 0.0)
    base = it.get("base", 0.0)
    r = r * ((it["height"] - base) / r[mask].max())
    if base > 0:
        return np.where(mask, r + base, np.nan), px_mm
    return np.where(mask & (r > 0.02), r, np.nan), px_mm


def resample(r, px_mm, cell):
    h, w = r.shape
    nw, nh = int(round(w * px_mm / cell)), int(round(h * px_mm / cell))
    filled = np.where(np.isfinite(r), r, 0.0)
    yy = (np.arange(nh) + 0.5) * cell / px_mm - 0.5
    xx = (np.arange(nw) + 0.5) * cell / px_mm - 0.5
    Y, X = np.meshgrid(yy, xx, indexing="ij")
    v = ndimage.map_coordinates(filled, [Y, X], order=3, mode="nearest")
    msk = ndimage.map_coordinates(np.isfinite(r).astype(float), [Y, X], order=1, mode="constant")
    return np.where((msk > 0.5) & (v > 0.02), np.maximum(v, 0), np.nan)


STEP = 0.002


def encode16(r):
    """16 bit PNG: 0 is no material, v above 0 is (v - 1) * STEP millimetres."""
    q = np.where(np.isfinite(r), 1 + np.round(np.clip(r, 0, None) / STEP), 0)
    assert q.max() <= 65535
    q = q.astype(np.uint16)
    im = Image.fromarray(q[::-1].copy())
    buf = io.BytesIO()
    im.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def make_relief(it, dl):
    mesh = os.path.join(dl, it["file"])
    z, m = depth_map(mesh, it.get("px", 1200), it.get("rot", [0, 0, 0]), it.get("renormal", False))
    r, px_mm = bas_relief(z, m["cell"], it)
    store = it.get("cell", 0.2)
    r = resample(r, px_mm, store)
    r = ndimage.median_filter(np.where(np.isfinite(r), r, 0), 3) * np.isfinite(r) + np.where(np.isfinite(r), 0, np.nan)
    return encode16(r), {"step": STEP, "cell": store}


def make_tile(it, dl):
    """A seamless tile from a displacement map, resized with area averaging."""
    im = Image.open(os.path.join(dl, it["file"]))
    a = np.asarray(im).astype(np.float64)
    if a.ndim == 3:
        a = a[..., 0]
    a = a / (65535.0 if a.max() > 255 else 255.0)
    crop = it.get("crop")
    if crop:
        h, w = a.shape
        a = a[int(crop[1] * h):int(crop[3] * h), int(crop[0] * w):int(crop[2] * w)]
    px = it.get("px", 512)
    py = int(round(px * a.shape[0] / a.shape[1]))
    img = Image.fromarray(a.astype(np.float32), mode="F").resize((px, py), Image.Resampling.BOX)
    a = np.asarray(img).astype(np.float64)
    if it.get("invert"):
        a = 1.0 - a
    lo, hi = np.percentile(a, 0.2), np.percentile(a, 99.8)
    a = np.clip((a - lo) / (hi - lo), 0, 1)
    height = it.get("height", 2.0)
    r = a * height
    # A light wrapped blur takes out single pixel noise the source carries.
    r = ndimage.gaussian_filter(r, it.get("blur", 0.7), mode="wrap")
    r = r[::-1]  # rows from the lowest Y up
    r = np.maximum(r - r.min(), 0) + 0.05
    return encode16(r), {"step": STEP, "cell": it["size"] / px}


def make_shape(it, dl):
    text = open(os.path.join(dl, it["file"]), encoding="utf-8").read()
    # Keep only what the importer reads: strip metadata, comments and titles.
    text = re.sub(r"<\?xml[^>]*>", "", text)
    text = re.sub(r"<!DOCTYPE[^>]*>", "", text)
    text = re.sub(r"<!--.*?-->", "", text, flags=re.S)
    text = re.sub(r"<(metadata|title|desc)\b.*?</\1>", "", text, flags=re.S)
    text = re.sub(r"\s+", " ", text).strip() + "\n"
    return text.encode("utf-8"), {"size": it.get("size", 100.0)}


def rust_str(s):
    out = json.dumps(s, ensure_ascii=False)
    return re.sub(r"[^\x00-\x7f]", lambda m: "\\u{%x}" % ord(m.group(0)), out)


def main():
    items_path, dl, repo = sys.argv[1:4]
    only = sys.argv[4] if len(sys.argv) > 4 else None
    items = json.load(open(items_path))
    assets = os.path.join(repo, "crates/carve-core/assets/clipart/imported")
    cache_path = os.path.join(dl, "conversions.json")
    cache = json.load(open(cache_path)) if os.path.exists(cache_path) else {}
    rows = []
    wanted = set()
    for it in items:
        ext = "svg" if it["kind"] == "shape" else "png"
        rel = f"{it['set']}/{slug(it['name'])}.{ext}"
        wanted.add(rel)
        out = os.path.join(assets, rel)
        key = json.dumps({k: v for k, v in it.items() if k not in ("tags", "category")}, sort_keys=True)
        if (only is None or re.search(only, it["name"])) and (cache.get(rel, {}).get("key") != key or not os.path.exists(out)):
            maker = {"relief": make_relief, "tile": make_tile, "shape": make_shape}[it["kind"]]
            data, form = maker(it, dl)
            os.makedirs(os.path.dirname(out), exist_ok=True)
            open(out, "wb").write(data)
            cache[rel] = {"key": key, "form": form}
            print(f"{rel}: {len(data) // 1024} KB")
        rows.append((it, rel, cache[rel]["form"]))
    json.dump(cache, open(cache_path, "w"), indent=1)
    for root, _, files in os.walk(assets):
        for f in files:
            rel = os.path.relpath(os.path.join(root, f), assets)
            if rel.endswith((".png", ".svg")) and rel not in wanted:
                os.remove(os.path.join(root, f))
                print(f"removed {rel}")
    lines = [
        "//! The imported items, written by tools/clipart-import/clipimport.py from",
        "//! tools/clipart-import/items.json. Change those and run it again rather",
        "//! than editing this file.",
        "",
        "use super::{Form, Item, Origin};",
        "",
        "pub(super) const ITEMS: &[Item] = &[",
    ]
    for it, rel, form in rows:
        if it["kind"] == "relief":
            f = f"Form::Relief {{ step: {form['step']}, cell: {form['cell']} }}"
        elif it["kind"] == "tile":
            f = f"Form::Tile {{ step: {form['step']}, cell: {form['cell']:.6f} }}"
        else:
            f = f"Form::Shape {{ size: {float(form['size']):.1f} }}"
        tags = ", ".join(rust_str(t) for t in it["tags"])
        lines += [
            "    Item {",
            f"        category: {rust_str(it['category'])},",
            f"        name: {rust_str(it['name'])},",
            f"        tags: &[{tags}],",
            f"        form: {f},",
            f"        data: include_bytes!(\"../../../../assets/clipart/imported/{rel}\"),",
            "        origin: Origin {",
            f"            url: {rust_str(it['url'])},",
            f"            author: {rust_str(it['author'])},",
            f"            license: {rust_str(it['license'])},",
            f"            downloaded: {rust_str(it['downloaded'])},",
            "        },",
            "    },",
        ]
    lines.append("];")
    target = os.path.join(repo, "crates/carve-core/src/clipart/library/imported/items.rs")
    open(target, "w").write("\n".join(lines) + "\n")
    subprocess.run(["rustfmt", "--edition", "2021", target], check=False)
    total = sum(os.path.getsize(os.path.join(r, f)) for r, _, fs in os.walk(assets) for f in fs)
    print(f"{len(rows)} items, {total / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
