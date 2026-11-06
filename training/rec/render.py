"""Synthetic text lines for a recognizer, drawn as the target screen shows them, cut like the text detector cuts
them from a frame (at native pixel size: the loader scales them to the input height like the engine).

Everything screen-specific comes from the recipe's `synth` section (see training/README.md):
- `kinds`: the mix: `{name, w, style, text: <text spec>}` or `{..., label_value: {...}}` (a label, a gap and a
  value, optionally coloured and with a ▲/▼ marker);
- `styles`: per style the fonts (by weight), cap heights (px), colours, background banks, decorations (header
  bars, flat panels, coloured boxes behind the text, two-coloured lines) and neighbouring lines;
- `fonts`: the font folder, groups (stretch and stroke ranges per group), caps-only fonts, the fallback;
- `colours`: names -> RGB; `backgrounds`: banks of background images; `render`: supersampling, tracking, line
  pitch, opacity, blur/noise/JPEG, the crop's margins.
The texts: training/rec/texts.py.
"""
import math
import multiprocessing as mp
import os
import random
import time

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont

from ..common.datasets import read_lines
from .texts import Texts


def _imread(path):
    return cv2.imdecode(np.fromfile(path, np.uint8), cv2.IMREAD_COLOR)


def _draw(rng, d: dict):
    """A random value from `{gauss: [mu, sigma]}` or `{uniform: [lo, hi]}`."""
    if "gauss" in d:
        return rng.gauss(*d["gauss"])
    return rng.uniform(*d["uniform"])


def _pick(rng, items: list[dict]):
    return rng.choices(items, weights=[i.get("w", 1) for i in items])[0]


class Mix:
    """Draws line specs from the recipe's kinds."""

    def __init__(self, cfg: dict, texts: Texts):
        self.kinds = cfg["kinds"]
        self.by_name = {k["name"]: k for k in self.kinds}
        self.texts = texts

    def sample(self, rng, force: str | None = None) -> dict:
        kind = self.by_name[force] if force else _pick(rng, self.kinds)
        for _ in range(50):
            spec = self._spec(kind, rng)
            if spec:
                return spec
        raise RuntimeError(f"kind {kind['name']}: its text spec yields nothing")

    def _spec(self, kind: dict, rng) -> dict | None:
        base = {"kind": kind["name"], "style": kind["style"]}
        if "text" in kind:
            text = self.texts.gen(kind["text"], rng)
            return dict(base, text=text) if text else None
        lv = kind["label_value"]
        labels = list(lv["labels"])
        label = rng.choices(labels, weights=[lv["labels"][k] for k in labels])[0]
        alt = _pick(rng, lv["values"].get(label) or lv["values"]["default"])
        value = self.texts.template(alt["t"], rng)
        tri, colour, vcolour = None, None, None
        lc, vc = lv.get("line_colour"), lv.get("value_colour")
        y = rng.random()
        if lc and y < lc["prob"]:
            good = rng.random() < lc["good"]
            colour = rng.choice(lc["good_colours"] if good else lc["bad_colours"])
            if alt.get("tri") and rng.random() < lc["tri"]:
                tri = lc["tri_up"] if good else lc["tri_down"]
        elif vc and y < (lc["prob"] if lc else 0) + vc["prob"]:
            vcolour = rng.choice(vc["colours"])
        return dict(base, label=label + lv.get("suffix", ":"), value=value, tri=tri, colour=colour, vcolour=vcolour,
                    column=rng.random() < lv["column"], gap=lv["gap"], tri_gap=lv["tri_gap"])

    @staticmethod
    def line(spec: dict) -> str:
        if "text" in spec:
            return spec["text"]
        return f"{spec['label']} {spec['value']}" + (f" {spec['tri']}" if spec["tri"] else "")


class Renderer:
    def __init__(self, cfg: dict, recipe, texts: Texts):
        from fontTools.ttLib import TTFont

        self.cfg = cfg
        self.r = cfg["render"]
        self.texts = texts
        self.mix = Mix(cfg, texts)
        self.colours = {k: tuple(v) for k, v in cfg["colours"].items()}
        f = cfg["fonts"]
        self.font_dir = recipe.path(f["dir"])
        self.fallback = f["fallback"]
        self.caps_only = set(f.get("caps_only", []))
        self.group_of, self.groups = {}, f["groups"]
        for g, spec in self.groups.items():
            for name in spec["fonts"]:
                self.group_of[name] = g
        self.cmap, self.cap_ratio = {}, {}
        for name in self.group_of:
            path = f"{self.font_dir}/{name}.ttf"
            self.cmap[name] = set(TTFont(path).getBestCmap())
            b = ImageFont.truetype(path, 200).getbbox("H")
            self.cap_ratio[name] = (b[3] - b[1]) / 200
        self.banks = {}
        for name, b in cfg["backgrounds"].items():
            files = [p for pat in ([b["glob"]] if isinstance(b["glob"], str) else b["glob"]) for p in recipe.glob(pat)]
            if not files:
                raise FileNotFoundError(f"background bank {name}: no images for {b['glob']} (build it first)")
            self.banks[name] = (files, b)
        self.triangles = self.r.get("triangles", {})
        self.fonts, self.adv = {}, {}

    # ------------------------------------------------------------ fonts

    def covers(self, name, text):
        cm = self.cmap[name]
        return all(ord(c) in cm or c in self.triangles for c in text)

    def font(self, name, px):
        key = (name, px)
        f = self.fonts.get(key)
        if f is None:
            f = self.fonts[key] = ImageFont.truetype(f"{self.font_dir}/{name}.ttf", px)
        return f

    def advance(self, name, px, ch):
        key = (name, px, ch)
        a = self.adv.get(key)
        if a is None:
            a = self.adv[key] = self.font(name, px).getlength(ch)
        return a

    def pick_font(self, rng, style, text) -> dict:
        e = _pick(rng, style["fonts"])
        name = e["font"] if "font" in e else rng.choice(self.groups[e["group"]]["fonts"])
        if not self.covers(name, text):
            name, e = self.fallback, {}
        g = self.groups[self.group_of[name]]
        return {"name": name, "hscale": e.get("hscale", g["hscale"]), "stroke": e.get("stroke", g["stroke"])}

    def draw_run(self, draw, fname, px, x, base, text, track, cap, stroke=0.0):
        """Draws character by character (tracking); ▲▼ as polygons. Returns the end x (supersampled px)."""
        f = self.font(fname, px)
        for ch in text:
            shape = self.triangles.get(ch)
            if shape:
                w, h = 1.05 * cap, 0.6 * cap
                cy = base - cap + 0.45 * cap
                if shape == "up":
                    pts = [(x, cy + h / 2), (x + w, cy + h / 2), (x + w / 2, cy - h / 2)]
                else:
                    pts = [(x, cy - h / 2), (x + w, cy - h / 2), (x + w / 2, cy + h / 2)]
                draw.polygon(pts, fill=255)
                if stroke > 0:
                    draw.line(pts + pts[:1], fill=255, width=max(1, round(2 * stroke)))
                x += w + track
            else:
                draw.text((round(x), round(base)), ch, font=f, fill=255, anchor="ls", stroke_width=stroke, stroke_fill=255)
                x += self.advance(fname, px, ch) + track
        return x

    # ------------------------------------------------------------ backgrounds

    def background(self, rng, nrng, style, w, h, cap):
        rule = next(b for b in style["background"] if cap <= b.get("max_cap", math.inf))
        files, b = self.banks[rule["bank"]]
        img = _imread(rng.choice(files))
        H, W = img.shape[:2]
        if W < w or H < h:
            s = max(w / W, h / H) * 1.05
            img = cv2.resize(img, (math.ceil(W * s), math.ceil(H * s)), interpolation=cv2.INTER_LINEAR)
            H, W = img.shape[:2]
        x, y = rng.randint(0, W - w), rng.randint(0, H - h)
        p = img[y:y + h, x:x + w].astype(np.float32)
        if rng.random() < b.get("flip", 0):
            p = p[:, ::-1]
        j = b.get("jitter")
        if j and rng.random() < j["prob"]:
            p = p * rng.uniform(*j["gain"]) + rng.uniform(*j["offset"])
        return np.clip(p, 0, 255)

    def colour(self, name, rng, scale=1.0):
        c = np.array(self.colours[name][::-1], np.float32) * scale     # BGR
        return np.minimum(255, c * rng.uniform(*self.r["colour_jitter"]))

    # ------------------------------------------------------------ one line

    def render(self, rng, nrng, force=None):
        spec = self.mix.sample(rng, force)
        style = self.cfg["styles"][spec["style"]]
        label = Mix.line(spec)
        fe = self.pick_font(rng, style, label)
        fname = fe["name"]
        if fname in self.caps_only:                       # its lower case is upper case: the label too
            u = self.texts.upper(label)
            if u != label.upper() or not self.covers(fname, u):
                fname = self.fallback
                g = self.groups[self.group_of[fname]]
                fe = {"name": fname, "hscale": g["hscale"], "stroke": g["stroke"]}
            else:
                label = u
                if "text" in spec:
                    spec = dict(spec, text=u)
                else:
                    spec = dict(spec, label=spec["label"].upper(), value=spec["value"].upper())
        cap = max(self.r["min_cap"], _draw(rng, _pick(rng, style["cap"])))
        SS = self.r["supersample"]
        ratio = self.cap_ratio[fname]
        px = max(6, round(cap / ratio * SS))
        cap_ss = px * ratio
        hscale = rng.uniform(*fe["hscale"])
        stroke = rng.uniform(*fe["stroke"]) * cap_ss
        track = rng.uniform(*self.r["track"]) * cap_ss
        space = self.advance(fname, px, " ")

        # Colours: the line's, the value's (label_value) or the second part's (split)
        c = _pick(rng, style["colours"])
        colour_name, colour_scale = c["colour"], c.get("scale", 1.0)
        vcolour_name = None
        if "label" in spec:
            if spec["colour"]:
                colour_name, colour_scale = spec["colour"], 1.0
            vcolour_name = spec["vcolour"]
        split = style.get("split")
        split = bool(split) and " " in label and rng.random() < split["prob"]
        if split:
            colour_name, vcolour_name, colour_scale = style["split"]["colours"][0], style["split"]["colours"][1], 1.0
        dim = style.get("dim")
        if dim and rng.random() < dim["prob"]:
            colour_scale *= rng.uniform(*dim["factor"])
        colour = self.colour(colour_name, rng, colour_scale)
        vcol = colour if vcolour_name is None else self.colour(vcolour_name, rng)

        pitch = rng.uniform(*self.r["pitch"]) * cap_ss
        est_w = int(sum(self.advance(fname, px, ch) + track for ch in label) + 12 * cap_ss + 2 * space)
        pad = int(2.5 * cap_ss)
        W = est_w + 2 * pad
        H = int(4 * pitch + 2 * cap_ss)
        base = int(H / 2 + cap_ss / 2)
        x0 = pad + rng.uniform(0, 2 * SS)

        main, val = Image.new("L", (W, H), 0), Image.new("L", (W, H), 0)
        d, dv = ImageDraw.Draw(main), ImageDraw.Draw(val)
        text_end = None
        if "label" in spec:
            xe = self.draw_run(d, fname, px, x0, base, spec["label"], track, cap_ss, stroke)
            gap = space if not spec["column"] else space + rng.uniform(*spec["gap"]) * cap_ss
            xe = self.draw_run(dv, fname, px, xe + gap, base, spec["value"], track, cap_ss, stroke)
            if spec["tri"]:
                self.draw_run(dv, fname, px, xe + rng.uniform(*spec["tri_gap"]) * cap_ss, base, spec["tri"], track,
                              cap_ss, stroke)
        elif split:
            head, tail = label.split(" ", 1)
            xe = self.draw_run(d, fname, px, x0, base, head, track, cap_ss, stroke)
            text_end = self.draw_run(dv, fname, px, xe + space + track, base, tail, track, cap_ss, stroke)
        else:
            text_end = self.draw_run(d, fname, px, x0, base, label, track, cap_ss, stroke)

        # Neighbouring lines above and below (their descenders reach into real crops)
        nb = Image.new("L", (W, H), 0)
        dn = ImageDraw.Draw(nb)
        ns = style.get("neighbours")
        for sgn in (-1, 1):
            if ns and rng.random() < ns["prob"]:
                t = Mix.line(self.mix.sample(rng, ns.get("from")))
                n = rng.randint(min(3, len(t)), len(t)) if rng.random() < self.r["partial_neighbour"] else len(t)
                t = t.upper() if fname in self.caps_only else t
                if self.covers(fname, t[:n]):
                    self.draw_run(dn, fname, px, x0, base + sgn * pitch, t[:n], track, cap_ss, stroke)

        def down(im):
            a = np.asarray(im, np.float32)
            return cv2.resize(a, (max(1, round(W * hscale / SS)), H // SS), interpolation=cv2.INTER_AREA) / 255.0
        am, av, an = down(main), down(val), down(nb)
        h, w = am.shape
        cap_px = cap_ss / SS
        base_px = base / SS

        bg = self.background(rng, nrng, style, w, h, cap)
        y = rng.random()
        acc = 0.0
        bar = None
        for b in style.get("bars", []):
            acc += b["prob"]
            if y < acc:
                bar = b
                break
        flat = style.get("flat")
        if bar:                                                   # a header bar behind the line
            t0 = int(base_px - cap_px - rng.uniform(*bar["height"]) * cap_px)
            t1 = int(base_px + rng.uniform(*bar["height"]) * cap_px)
            barc = np.array(self.colours[bar["colour"]][::-1], np.float32) * rng.uniform(*bar["scale"])
            bg[max(0, t0):t1] = bg[max(0, t0):t1] * 0.15 + barc * 0.85
        elif flat and y < acc + flat["prob"]:                     # a plain panel
            bg = bg * 0 + np.array(flat["colour"][::-1], np.float32) * rng.uniform(*flat["scale"]) + \
                nrng.normal(0, flat["noise"], bg.shape)
        box = style.get("box")
        if box and text_end is not None:                          # a coloured box around the text (tags)
            tc = np.array(self.colours[rng.choice(box["colours"])][::-1], np.float32) * rng.uniform(*box["scale"])
            l0 = int(x0 * hscale / SS - rng.uniform(*box["pad_x"]) * cap_px)
            l1 = int(text_end * hscale / SS + rng.uniform(*box["pad_x"]) * cap_px)
            t0 = int(base_px - cap_px - rng.uniform(*box["pad_y"]) * cap_px)
            t1 = int(base_px + rng.uniform(*box["pad_y"]) * cap_px)
            bg[max(0, t0):t1, max(0, l0):l1] = bg[max(0, t0):t1, max(0, l0):l1] * 0.15 + tc * 0.85
        nb_col = np.array(self.colours[ns["colour"]][::-1], np.float32) * rng.uniform(*ns["scale"]) if ns else colour
        opacity = rng.uniform(*self.r["opacity"])
        img = bg * (1 - an[..., None] * opacity) + nb_col * an[..., None] * opacity
        img = img * (1 - am[..., None] * opacity) + colour * am[..., None] * opacity
        img = img * (1 - av[..., None] * opacity) + vcol * av[..., None] * opacity

        blur, noise, jpeg = self.r["blur"], self.r["noise"], self.r["jpeg"]
        if rng.random() < blur["prob"]:
            img = cv2.GaussianBlur(img, (0, 0), rng.uniform(*blur["sigma"]))
        if rng.random() < noise["prob"]:
            img = img + nrng.normal(0, rng.uniform(*noise["sigma"]), img.shape)
        img = np.clip(img + 0.5, 0, 255).astype(np.uint8)
        if rng.random() < jpeg["prob"]:
            img = cv2.imdecode(cv2.imencode(".jpg", img, [cv2.IMWRITE_JPEG_QUALITY, rng.randint(*jpeg["quality"])])[1],
                               cv2.IMREAD_COLOR)

        # The crop like the text detector's box: around the line's ink, margins relative to the cap height
        cols = np.where(np.maximum(am, av).max(0) > 0.25)[0]
        if len(cols) == 0:
            return None
        m = self.r["box"]

        def margin(k):
            s = m[k]
            return min(s["clip"][1], max(s["clip"][0], rng.gauss(*s["gauss"])))
        has_desc = any(ch in m["descender_chars"] for ch in label)
        mt = margin("top")
        mb = margin("bottom") + (rng.uniform(*m["bottom"]["descender_extra"]) if has_desc else 0)
        ml = margin("left")
        mr = margin("right")
        bx0 = math.floor(cols[0] - ml * cap_px)
        bx1 = math.ceil(cols[-1] + 1 + mr * cap_px)
        by0 = math.floor(base_px - cap_px - mt * cap_px)
        by1 = math.ceil(base_px + mb * cap_px)
        crop = img[max(0, by0):min(h, by1), max(0, bx0):min(w, bx1)]
        return crop, label, spec["kind"], fname


# ---------------------------------------------------------------- generation (worker pool)

_R = None
_SEED = 0


def _init(cfg, recipe, texts, seed):
    global _R, _SEED
    cv2.setNumThreads(1)
    _R = Renderer(cfg, recipe, texts)
    _SEED = seed


def _work(i):
    rng = random.Random(_SEED * 1_000_003 + i)
    nrng = np.random.default_rng([_SEED, i])
    while True:
        r = _R.render(rng, nrng)
        if r is not None and r[0].size and r[1]:
            break
    return i, cv2.imencode(".png", r[0], [cv2.IMWRITE_PNG_COMPRESSION, 9])[1].tobytes(), r[1], r[2], r[3]


def generate(cfg: dict, recipe, charset: set[str], n: int, out: str, seed: int, workers: int):
    """Writes n lines to `out` (subfolders of 10,000 PNGs, labels.tsv with the columns file, text, kind, font)."""
    t0 = time.time()
    texts = Texts(cfg, recipe, charset)
    print("pools: " + ", ".join(f"{k} {len(v)}" for k, v in texts.pools.items()), flush=True)
    os.makedirs(out, exist_ok=True)
    tmp = f"{out}/labels.tsv.part"
    with open(tmp, "w", encoding="utf-8", newline="\n") as lab, \
            mp.Pool(workers, _init, (cfg, recipe, texts, seed)) as pool:
        lab.write("file\ttext\tkind\tfont\n")
        for i, png, label, kind, font in pool.imap(_work, range(n), chunksize=200):
            sub = f"{i // 10000:02d}"
            if i % 10000 == 0:
                os.makedirs(f"{out}/{sub}", exist_ok=True)
                print(f"  {i} ({time.time() - t0:.0f} s)", flush=True)
            fn = f"{sub}/{i:06d}.png"
            with open(f"{out}/{fn}", "wb") as f:
                f.write(png)
            lab.write(f"{fn}\t{label}\t{kind}\t{font}\n")
    os.replace(tmp, f"{out}/labels.tsv")
    print(f"{n} lines in {time.time() - t0:.0f} s -> {out}", flush=True)


def sheet(path: str, synth_dir: str, real_dir: str | None, k: int = 30, seed: int = 0):
    """A contact sheet: k synthetic lines (left) next to k real ones (right), each at height 48, with labels."""
    rng = random.Random(seed)
    syn = rng.sample(read_lines(synth_dir), k)
    real = rng.sample(read_lines(real_dir), k) if real_dir else []
    colw = 760
    font = ImageFont.load_default()

    def cell(p, text):
        img = _imread(p)
        h, w = img.shape[:2]
        img = cv2.resize(img, (max(1, math.ceil(48 * w / h)), 48), interpolation=cv2.INTER_LINEAR)[:, :colw]
        c = Image.new("RGB", (colw, 68), (255, 255, 255))
        c.paste(Image.fromarray(img[..., ::-1]), (0, 0))
        ImageDraw.Draw(c).text((2, 52), f"{text}   [{h}px]", fill=(0, 110, 0), font=font)
        return c
    out = Image.new("RGB", (2 * colw + 20, 68 * k), (255, 255, 255))
    for i, r in enumerate(syn):
        out.paste(cell(r["file"], r["text"]), (0, 68 * i))
    for i, r in enumerate(real):
        out.paste(cell(r["file"], r["text"]), (colw + 20, 68 * i))
    out.save(path)
    print("sheet:", path)
