"""Recipes: one YAML file per model (`training/recipes/<id>.yml`).

Paths in a recipe are relative to the data root (`--data-root`, else `$SC_OHSEEARR_DATA`, else `<repo>/local`);
absolute paths are kept, `${VAR}` is replaced from the environment. `--set a.b=value` overrides a key (the value
is parsed as YAML).
"""
import glob
import os
import re

import yaml

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))).replace("\\", "/")
_MISSING = object()


def default_data_root() -> str:
    return (os.environ.get("SC_OHSEEARR_DATA") or f"{REPO}/local").replace("\\", "/")


class Recipe:
    def __init__(self, path: str, data_root: str | None = None, overrides: list[str] | None = None):
        self.file = os.path.abspath(path)
        with open(path, encoding="utf-8") as f:
            self.raw = yaml.safe_load(f)
        for o in overrides or []:
            key, _, value = o.partition("=")
            self._set(key, yaml.safe_load(value))
        self.id = self.raw["id"]
        self.family = self.raw["family"]
        self.data_root = (data_root or default_data_root()).replace("\\", "/")

    def _set(self, dotted: str, value):
        node = self.raw
        parts = dotted.split(".")
        for p in parts[:-1]:
            node = node.setdefault(p, {})
        node[parts[-1]] = value

    def get(self, dotted: str, default=_MISSING):
        node = self.raw
        for p in dotted.split("."):
            if not isinstance(node, dict) or p not in node:
                if default is _MISSING:
                    raise KeyError(f"recipe {self.id}: missing key {dotted}")
                return default
            node = node[p]
        return node

    def path(self, p: str) -> str:
        """A recipe path made absolute (relative to the data root)."""
        p = os.path.expandvars(p)
        if re.search(r"\$\{?\w+", p):
            raise ValueError(f"recipe {self.id}: unset environment variable in path {p!r}")
        p = p.replace("\\", "/")
        return p if os.path.isabs(p) else f"{self.data_root}/{p}"

    def glob(self, pattern: str) -> list[str]:
        return sorted(g.replace("\\", "/") for g in glob.glob(self.path(pattern)))

    def out_dir(self, out: str | None) -> str:
        """The run's folder: `--out`, else `<data root>/training/<id>`."""
        d = os.path.abspath(out) if out else f"{self.data_root}/training/{self.id}"
        os.makedirs(d, exist_ok=True)
        return d.replace("\\", "/")


def add_recipe_args(ap):
    ap.add_argument("recipe", help="recipe file (training/recipes/<id>.yml)")
    ap.add_argument("--data-root", help="where the recipe's relative paths point (default $SC_OHSEEARR_DATA or <repo>/local)")
    ap.add_argument("--set", action="append", default=[], metavar="KEY=VALUE", help="override a recipe key (YAML value)")
    ap.add_argument("--out", help="the run's folder (default <data root>/training/<id>)")
