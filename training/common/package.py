"""Writes a model package folder (ADR 0005): `model.onnx`, `model.toml`, family files (e.g. `dict.txt`), plus
the run's `metrics.json` and `MODEL.metrics.md` (a block to paste into the package's `MODEL.md`)."""
import datetime
import json
import os
import shutil
import subprocess

from .recipe import REPO, Recipe


def _value(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return repr(v)
    if isinstance(v, (list, tuple)):
        return "[" + ", ".join(_value(x) for x in v) + "]"
    return json.dumps(str(v), ensure_ascii=False)


def manifest(head: dict, sections: dict[str, dict]) -> str:
    """`model.toml` text: the head keys, then one table per section (in order)."""
    lines = [f"{k} = {_value(v)}" for k, v in head.items()]
    for name, table in sections.items():
        lines += ["", f"[{name}]"] + [f"{k} = {_value(v)}" for k, v in table.items()]
    return "\n".join(lines) + "\n"


def commit() -> str:
    try:
        return subprocess.run(["git", "-C", REPO, "rev-parse", "--short", "HEAD"], capture_output=True, text=True,
                              check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def head(recipe: Recipe) -> dict:
    """The manifest's head from the recipe's `package` section; `{date}`, `{commit}`, `{recipe}` in `source` are
    filled in."""
    p = recipe.get("package")
    source = p["source"].format(date=datetime.date.today().isoformat(), commit=commit(), recipe=recipe.id)
    return {"id": recipe.id, "version": p["version"], "family": recipe.family, "license": p["license"],
            "description": p["description"], "source": source}


def write(folder: str, recipe: Recipe, sections: dict[str, dict], files: dict[str, str], metrics: dict,
          metrics_md: str) -> str:
    """The package in `folder`: `files` maps package file names to sources (copied), `metrics` goes to
    metrics.json, `metrics_md` to MODEL.metrics.md."""
    os.makedirs(folder, exist_ok=True)
    for name, src in files.items():
        shutil.copyfile(src, os.path.join(folder, name))
    with open(os.path.join(folder, "model.toml"), "w", encoding="utf-8", newline="\n") as f:
        f.write(manifest(head(recipe), sections))
    with open(os.path.join(folder, "metrics.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(metrics, f, indent=1, ensure_ascii=False)
    with open(os.path.join(folder, "MODEL.metrics.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write(metrics_md.rstrip() + "\n")
    return folder
