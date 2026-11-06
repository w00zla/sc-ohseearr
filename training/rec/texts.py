"""Text sources for synthetic lines: pools of strings built from language files (the game's `global.ini` or any
`key=value` file) and the recipe's lists, and text specs that draw lines from them.

Recipe keys under `synth`:
- `packs`: name -> path of a `key=value` file (UTF-8, `\\n` inside values = line breaks).
- `char_map`: replacements applied to source texts (typographic quotes, dashes, ...).
- `reject`: regular expressions; a source text matching one is dropped.
- `corpora`: name -> how to build a pool:
  - `ini: {packs: [..], keys: [prefix..], exclude_keys: [substring..], lines: split|join|raw, min_len, max_len,
    contains, exclude_chars}`: the values of the packs whose (lower-case) key starts with a prefix;
  - `match: {from: pool, regex, group}`: the pool's entries matching `regex` (or its group);
  - `words: {from: [pool..], head, strip, min_len, max_len, capitalized, alpha}`: the words of pools' entries.
- `lists`: name -> a list of strings (a pool given verbatim).

A text spec (used by the kinds of the mix) is a template string or a dict:
- `{t: template | [templates..], with: {name: spec}}`: a template (a random one of a list); `{name}` of `with`
  is replaced by that spec's text;
- `{pick: pool, wrap: [min, max], need: substring}`: a random entry; with `wrap` word-wrapped at a random width
  and one of its lines (with `need`: one containing it);
- `{one_of: [{w: weight, ...spec}..]}`: one of several specs by weight;
- `{repeat: spec, n: [min, max], sep: " "}`;
- `{random: {min, max, unicode, space}}`: random characters of the charset (`unicode`: the share drawing from
  all of it instead of ASCII; `space`: the chance of a space after a character);
- any dict may add `upper: p` (upper case with chance p, when the charset keeps it).
Template functions: `{pick:pool}`, `{lower:pool}`, `{int:lo:hi[:format]}`, `{float:lo:hi[:format]}` (default
`.2f`), `{choice:a|b|c}`, `{sign}` (+ or -).
"""
import re

_FUNC = re.compile(r"\{([A-Za-z_][\w]*)(?::([^{}]*))?\}")


def parse_ini(path: str) -> dict[str, str]:
    out = {}
    with open(path, encoding="utf-8-sig", errors="replace") as f:
        for line in f:
            eq = line.find("=")
            if eq <= 0:
                continue
            key = line[:eq].strip()
            if key.lower().endswith(",p"):
                key = key[:-2]
            out[key] = line[eq + 1:].rstrip("\r\n")
    return out


def wrap(text: str, width: int) -> list[str]:
    """Word wrap by characters like the game's tooltips; hyphenated words may break after the "-"."""
    lines, cur, words, i = [], "", text.split(" "), 0
    while i < len(words):
        w = words[i]
        cand = w if not cur else cur + " " + w
        if len(cand) <= width:
            cur, i = cand, i + 1
            continue
        room = width - (len(cur) + 1 if cur else 0)
        cut = max((j for j, ch in enumerate(w[:-1]) if ch == "-" and j + 1 <= room), default=-1)
        if cut >= 0:
            head = w[:cut + 1]
            lines.append(head if not cur else cur + " " + head)
            cur, words[i] = "", w[cut + 1:]
        elif cur:
            lines.append(cur)
            cur = ""
        else:
            cur, i = w, i + 1      # a word longer than the line
    if cur:
        lines.append(cur)
    return lines


class Texts:
    def __init__(self, cfg: dict, recipe, charset: set[str]):
        self.charset = charset
        self.char_map = cfg.get("char_map", {})
        self.reject = [re.compile(r) for r in cfg.get("reject", [])]
        packs = {name: parse_ini(recipe.path(p)) for name, p in cfg.get("packs", {}).items()}
        self.pools: dict[str, list[str]] = {}
        for name, items in cfg.get("lists", {}).items():
            self.pools[name] = [str(x) for x in items]
        for name, spec in cfg.get("corpora", {}).items():
            self.pools[name] = self._build(spec, packs)
        exclude = set(cfg.get("random_exclude", ""))
        self.rand_chars = sorted(c for c in charset if c != " " and c not in exclude)
        self.ascii = [c for c in self.rand_chars if ord(c) < 128]

    # ------------------------------------------------------------ pools

    def clean(self, s: str) -> str | None:
        """A source text as a label in the charset; None when it cannot be."""
        s = re.sub(r"<[^>]*>", "", s)
        for a, b in self.char_map.items():
            s = s.replace(a, b)
        s = re.sub(r" +", " ", s).strip()
        if not s or any(r.search(s) for r in self.reject) or any(c not in self.charset for c in s):
            return None
        return s

    def _build(self, spec: dict, packs: dict) -> list[str]:
        if "ini" in spec:
            s = spec["ini"]
            keys = [k.lower() for k in s.get("keys", [])]
            excl = [k.lower() for k in s.get("exclude_keys", [])]
            lines = s.get("lines", "split")
            lo, hi = s.get("min_len", 1), s.get("max_len", 10**9)
            out = set()
            for pk in s["packs"]:
                for k, v in packs[pk].items():
                    kl = k.lower()
                    if (keys and not any(kl.startswith(p) for p in keys)) or any(x in kl for x in excl):
                        continue
                    parts = v.split("\\n") if lines == "split" else [v.replace("\\n", " ")] if lines == "join" else [v]
                    for p in parts:
                        c = self.clean(p)
                        if c and lo <= len(c) <= hi and (not s.get("contains") or s["contains"] in c) \
                                and not any(ch in c for ch in s.get("exclude_chars", "")):
                            out.add(c)
            return sorted(out)
        if "match" in spec:
            s = spec["match"]
            rx, g = re.compile(s["regex"]), s.get("group", 0)
            return sorted({m.group(g) for e in self.pools[s["from"]] if (m := rx.search(e))})
        if "words" in spec:
            s = spec["words"]
            strip, lo, hi = s.get("strip", ""), s.get("min_len", 1), s.get("max_len", 10**9)
            words = set()
            for src in s["from"]:
                for p in self.pools[src][: s.get("head")]:
                    for w in p.split(" "):
                        w = w.strip(strip)
                        if lo <= len(w) <= hi and (not s.get("capitalized") or w[0].isupper()) \
                                and (not s.get("alpha") or w.isalpha()):
                            words.add(w)
            return sorted(words)
        raise ValueError(f"unknown corpus spec {spec}")

    # ------------------------------------------------------------ specs

    def upper(self, s: str) -> str:
        u = s.upper()
        return u if self.clean(u) == u else s

    def template(self, t: str, rng, slots: dict | None = None) -> str | None:
        failed = False

        def sub(m):
            nonlocal failed
            name, arg = m.group(1), m.group(2)
            if slots and name in slots and arg is None:
                v = self.gen(slots[name], rng)
                failed = failed or v is None
                return v or ""
            if name == "pick":
                return rng.choice(self.pools[arg])
            if name == "lower":
                return rng.choice(self.pools[arg]).lower()
            if name == "choice":
                return rng.choice(arg.split("|"))
            if name == "sign":
                return rng.choice("+-")
            if name in ("int", "float"):
                a = arg.split(":")
                lo, hi = float(a[0]), float(a[1])
                if name == "int":
                    return format(rng.randint(int(lo), int(hi)), a[2] if len(a) > 2 else "d")
                return format(rng.uniform(lo, hi), a[2] if len(a) > 2 else ".2f")
            raise ValueError(f"unknown template function {m.group(0)}")
        out = _FUNC.sub(sub, t)
        return None if failed else out

    def gen(self, spec, rng) -> str | None:
        """A text from a spec (see the module doc); None when the spec found nothing."""
        if isinstance(spec, str):
            return self.template(spec, rng)
        if "t" in spec:
            t = spec["t"]
            text = self.template(rng.choice(t) if isinstance(t, list) else t, rng, spec.get("with"))
        elif "pick" in spec:
            text = rng.choice(self.pools[spec["pick"]])
            if "wrap" in spec:
                lines = wrap(text, rng.randint(*spec["wrap"]))
                if spec.get("need"):
                    lines = [line for line in lines if spec["need"] in line]
                text = rng.choice(lines) if lines else None
        elif "one_of" in spec:
            alts = spec["one_of"]
            text = self.gen(rng.choices(alts, weights=[a.get("w", 1) for a in alts])[0], rng)
        elif "repeat" in spec:
            parts = [self.gen(spec["repeat"], rng) for _ in range(rng.randint(*spec["n"]))]
            text = None if None in parts else spec.get("sep", " ").join(parts)
        elif "random" in spec:
            text = self.random_line(rng, **spec["random"])
        else:
            raise ValueError(f"unknown text spec {spec}")
        if text is not None and spec.get("upper") and rng.random() < spec["upper"]:
            text = self.upper(text)
        return text

    def random_line(self, rng, min=3, max=40, unicode=0.6, space=0.18) -> str:
        n = rng.randint(min, max)
        pool = self.rand_chars if rng.random() < unicode else self.ascii
        out, wl = [], 0
        while len(out) < n:
            if wl > 0 and rng.random() < space and len(out) < n - 1:
                out.append(" ")
                wl = 0
                continue
            out.append(rng.choice(pool))
            wl += 1
        return re.sub(r" +", " ", "".join(out)).strip()
