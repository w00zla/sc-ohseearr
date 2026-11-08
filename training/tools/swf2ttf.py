"""The DefineFont3 fonts of a SWF (Star Citizen's Scaleform font library `Data/UI/fonts/Shared/fonts_en.swf`) ->
one TrueType file per font, for the recognizer recipes that draw the game's UI (`synth.fonts.dir`).

Reads each font's glyph shapes (EM 20480, scaled to 2048 units per em), advances, ascent/descent/leading and
kerning, and the names of its DefineFontName tag; writes `<out>/<Family>-<Style>.ttf` (e.g.
`BlenderProMedium-Regular.ttf`, the names the recipes use) with fontTools. The fonts are the game's and partly under
third-party licences (Blender Pro is commercial): use them for local training only, never put them into git or a
build. `scripts/extract-fonts.{ps1,sh}` extracts the SWF from your own Data.p4k and runs this.

  PY training/tools/swf2ttf.py <fonts_en.swf> <out dir>
PY = local/detector/.venv/Scripts/python.exe -I (from the repository root).
"""
import os
import struct
import sys
import zlib

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import newTable
from fontTools.ttLib.tables._k_e_r_n import KernTable_format_0

SCALE = 10  # DefineFont3: EM = 20480, target 2048 units per em
DEFINE_FONT_NAME, DEFINE_FONT3 = 88, 75


def load(path):
    """The SWF's body after the 8-byte header (zlib-compressed CWS/CFX or plain FWS/GFX)."""
    d = open(path, "rb").read()
    sig = d[:3]
    if sig in (b"CWS", b"CFX"):
        return zlib.decompress(d[8:])
    if sig in (b"FWS", b"GFX"):
        return d[8:]
    raise SystemExit(f"{path}: not a SWF (signature {sig!r})")


def tags(body):
    """(code, data) of every tag after the frame rect, frame rate and count."""
    nbits = body[0] >> 3
    p = (5 + 4 * nbits + 7) // 8 + 4
    while p < len(body):
        code_len = struct.unpack_from("<H", body, p)[0]
        p += 2
        code, ln = code_len >> 6, code_len & 0x3F
        if ln == 0x3F:
            ln = struct.unpack_from("<I", body, p)[0]
            p += 4
        yield code, body[p:p + ln]
        p += ln
        if code == 0:
            break


def cstr(b, p):
    """The zero-terminated string at `p` and the position after it."""
    e = b.index(0, p)
    return b[p:e].decode("utf-8", "replace"), e + 1


class Bits:
    def __init__(self, b, p):
        self.b, self.pos = b, p * 8

    def ub(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.b[self.pos >> 3] >> (7 - (self.pos & 7))) & 1)
            self.pos += 1
        return v

    def sb(self, n):
        if n == 0:
            return 0
        v = self.ub(n)
        return v - (1 << n) if v & (1 << (n - 1)) else v


def parse_shape(b, p):
    """A glyph's contours as lists of points and ('Q', control, end) segments."""
    r = Bits(b, p)
    nfill, nline = r.ub(4), r.ub(4)
    x = y = 0
    contours, cur = [], None
    while True:
        if r.ub(1) == 0:
            flags = r.ub(5)
            if flags == 0:
                break
            if flags & 1:  # move to
                nb = r.ub(5)
                x, y = r.sb(nb), r.sb(nb)
                cur = [(x, y)]
                contours.append(cur)
            if flags & 2:
                r.ub(nfill)
            if flags & 4:
                r.ub(nfill)
            if flags & 8:
                r.ub(nline)
            if flags & 16:
                raise ValueError("new styles in a font shape")
        else:
            if cur is None:
                cur = [(x, y)]
                contours.append(cur)
            if r.ub(1):  # straight edge
                nb = r.ub(4) + 2
                if r.ub(1):
                    dx, dy = r.sb(nb), r.sb(nb)
                elif r.ub(1):
                    dx, dy = 0, r.sb(nb)
                else:
                    dx, dy = r.sb(nb), 0
                x += dx
                y += dy
                cur.append((x, y))
            else:  # quadratic curve
                nb = r.ub(4) + 2
                cx, cy = x + r.sb(nb), y + r.sb(nb)
                x, y = cx + r.sb(nb), cy + r.sb(nb)
                cur.append(("Q", (cx, cy), (x, y)))
    return contours


def draw(pen, contours):
    t = lambda pt: (round(pt[0] / SCALE), round(-pt[1] / SCALE))  # noqa: E731
    for c in contours:
        if len(c) < 3:
            continue
        start = c[0]
        last = c[-1][2] if isinstance(c[-1][0], str) else c[-1]
        segs = c[1:]
        # A closed contour: drop the doubled end point (a straight line back to the start)
        if last == start and not isinstance(segs[-1][0], str):
            segs = segs[:-1]
        pen.moveTo(t(start))
        for s in segs:
            if isinstance(s[0], str):
                pen.qCurveTo(t(s[1]), t(s[2]))
            else:
                pen.lineTo(t(s))
        pen.closePath()


def convert(data, names, out_dir):
    fid, flags = struct.unpack_from("<HB", data, 0)
    nl = data[4]
    family = data[5:5 + nl].split(b"\0")[0].decode("latin-1")
    p = 5 + nl
    n = struct.unpack_from("<H", data, p)[0]
    p += 2
    wide_off, wide_codes, has_layout = flags & 8, flags & 4, flags & 128
    base = p
    fmt = "<I" if wide_off else "<H"
    sz = 4 if wide_off else 2
    offs = [struct.unpack_from(fmt, data, p + i * sz)[0] for i in range(n)]
    code_off = struct.unpack_from(fmt, data, p + n * sz)[0]
    shapes = [parse_shape(data, base + o) for o in offs]
    q = base + code_off
    codes = [struct.unpack_from("<H" if wide_codes else "<B", data, q + i * (2 if wide_codes else 1))[0] for i in range(n)]
    q += n * (2 if wide_codes else 1)
    ascent = descent = leading = 0
    adv = [0] * n
    kerns = []
    if has_layout:
        ascent, descent, leading = struct.unpack_from("<hhh", data, q)
        q += 6
        # SI16 by the spec, but values above 32767 occur (wide glyphs)
        adv = list(struct.unpack_from("<%dH" % n, data, q))
        q += 2 * n
        for _ in range(n):  # skip the bounds RECTs
            nb = data[q] >> 3
            q += (5 + 4 * nb + 7) // 8
        kc = struct.unpack_from("<H", data, q)[0]
        q += 2
        for _ in range(kc):
            if wide_codes:
                a, b, k = struct.unpack_from("<HHh", data, q)
                q += 6
            else:
                a, b, k = struct.unpack_from("<BBh", data, q)
                q += 4
            kerns.append((a, b, k))
    full, copyright = names.get(fid, (family, ""))
    style = full[len(family):].strip() if full.startswith(family) and full != family else ("Bold" if flags & 1 else "Regular")
    if full == family and flags & 1:
        style = "Bold"
    gnames = [".notdef"] + ["uni%04X" % c for c in codes]
    fb = FontBuilder(2048, isTTF=True)
    fb.setupGlyphOrder(gnames)
    fb.setupCharacterMap({c: "uni%04X" % c for c in codes})
    glyphs, metrics = {}, {}
    pen = TTGlyphPen(None)
    glyphs[".notdef"] = pen.glyph()
    metrics[".notdef"] = (1024, 0)
    for i, c in enumerate(codes):
        pen = TTGlyphPen(None)
        draw(pen, shapes[i])
        glyphs[gnames[i + 1]] = pen.glyph()
        metrics[gnames[i + 1]] = (round(adv[i] / SCALE), 0)
    fb.setupGlyf(glyphs)
    for gn, (a, _) in metrics.items():
        g = glyphs[gn]
        g.recalcBounds(fb.font["glyf"])
        metrics[gn] = (a, getattr(g, "xMin", 0) or 0)
    fb.setupHorizontalMetrics(metrics)
    asc, desc = round(ascent / SCALE), round(descent / SCALE)
    fb.setupHorizontalHeader(ascent=asc, descent=-desc, lineGap=round(leading / SCALE))
    ps = (family + "-" + style).replace(" ", "")
    fb.setupNameTable({"familyName": family, "styleName": style, "copyright": copyright.replace("\r", "\n"),
                       "uniqueFontIdentifier": ps + " (from SC fonts_en.swf)", "fullName": full, "psName": ps})
    fb.setupOS2(sTypoAscender=asc, sTypoDescender=-desc, sTypoLineGap=round(leading / SCALE),
                usWinAscent=asc, usWinDescent=desc)
    fb.setupPost()
    if kerns:
        kern = newTable("kern")
        kern.version = 0
        st = KernTable_format_0()
        st.version, st.coverage, st.format = 0, 1, 0
        # The pairs are character codes; pairs with a character without a glyph are dropped
        cs = set(codes)
        st.kernTable = {("uni%04X" % a, "uni%04X" % b): round(k / SCALE) for a, b, k in kerns if a in cs and b in cs}
        kern.kernTables = [st]
        fb.font["kern"] = kern
    path = os.path.join(out_dir, ps + ".ttf")
    fb.save(path)
    return path, n, len(kerns)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    src, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    names, fonts = {}, []
    for code, data in tags(load(src)):
        if code == DEFINE_FONT_NAME:
            fid = struct.unpack_from("<H", data, 0)[0]
            nm, p = cstr(data, 2)
            cr, _ = cstr(data, p)
            names[fid] = (nm, cr)
        elif code == DEFINE_FONT3:
            fonts.append(data)
    for d in fonts:
        path, glyphs, kerns = convert(d, names, out)
        print(f"{path}: {glyphs} glyphs, {kerns} kerning pairs")
    print(f"{len(fonts)} fonts -> {out}")


if __name__ == "__main__":
    main()
