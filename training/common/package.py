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


def copy_onnx(src: str, dst: str):
    """`src` to `dst` without doc strings and metadata: PyTorch's export writes the trace with the exporting
    machine's source paths into every node's doc string. The weights and the graph stay as they are."""
    import onnx

    m = onnx.load(src)
    for n in m.graph.node:
        n.doc_string = ""
    for v in [*m.graph.input, *m.graph.output, *m.graph.value_info, *m.graph.initializer]:
        v.doc_string = ""
    m.graph.doc_string = m.doc_string = ""
    del m.metadata_props[:]
    onnx.save(m, dst)


def write(folder: str, recipe: Recipe, sections: dict[str, dict], files: dict[str, str], metrics: dict,
          metrics_md: str) -> str:
    """The package in `folder`: `files` maps package file names to sources (copied; an `.onnx` without its doc
    strings and metadata), `metrics` goes to metrics.json, `metrics_md` to MODEL.metrics.md."""
    os.makedirs(folder, exist_ok=True)
    for name, src in files.items():
        if name.endswith(".onnx"):
            copy_onnx(src, os.path.join(folder, name))
        else:
            shutil.copyfile(src, os.path.join(folder, name))
    with open(os.path.join(folder, "model.toml"), "w", encoding="utf-8", newline="\n") as f:
        f.write(manifest(head(recipe), sections))
    with open(os.path.join(folder, "metrics.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(metrics, f, indent=1, ensure_ascii=False)
    with open(os.path.join(folder, "MODEL.metrics.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write(metrics_md.rstrip() + "\n")
    return folder
