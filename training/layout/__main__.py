"""The layout pipeline (family `nanodet`): recipe + COCO dataset -> model package.

  PY training/layout <recipe> [--steps config,train,export,eval,package] [--out <dir>]
  PY training/layout eval <recipe> --package <dir> [--split val]
  PY training/layout labels <recipe> [--split val] [--n 6]          contact sheet: windows with their labels
  PY training/layout errors <recipe> --package <dir> [--n 6]        contact sheet: false hits (red), misses (white)
  PY training/layout shots <recipe> --package <dir> <image>...      the model over screenshots without labels

PY = local/detector/.venv/Scripts/python.exe -I (from the repository root). Steps of a run:
  config   <out>/nanodet.yml from training/layout/<arch>.yml and the recipe
  train    NanoDet's tools/train.py (in the recipe's NanoDet checkout): <out>/nanodet/ (checkpoints, logs)
  export   NanoDet's tools/export_onnx.py (+ onnxsim): <out>/model.onnx
  eval     the recipe's eval split on <out>/model.onnx: <out>/eval.json
  package  <out>/package: model.onnx, model.toml, metrics.json, MODEL.metrics.md
"""
import argparse
import json
import os
import subprocess
import sys
import time

if __package__ in (None, ""):                       # run as `python training/layout`: import as a package
    _root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    sys.path = [p for p in sys.path if os.path.abspath(p) != os.path.dirname(os.path.abspath(__file__))]
    sys.path.insert(0, _root)
    __package__ = "training.layout"

import yaml  # noqa: E402

from ..common import package as pkg  # noqa: E402
from ..common.recipe import Recipe, add_recipe_args  # noqa: E402

STEPS = ["config", "train", "export", "eval", "package"]
COMMANDS = ["run", "eval", "labels", "errors", "shots"]
HERE = os.path.dirname(os.path.abspath(__file__)).replace("\\", "/")


def config(recipe: Recipe, out: str) -> dict:
    """The NanoDet config: the architecture template with the recipe's classes, input, data and schedule."""
    with open(f"{HERE}/{recipe.get('arch')}.yml", encoding="utf-8") as f:
        cfg = yaml.safe_load(f)
    t = recipe.get("train")
    data = recipe.path(recipe.get("data"))
    classes = recipe.get("classes")
    size = [recipe.get("input.width"), recipe.get("input.height")]
    norm = [recipe.get("normalize.mean"), recipe.get("normalize.std")]
    cfg["save_dir"] = f"{out}/nanodet"
    cfg["class_names"] = classes
    cfg["model"]["arch"]["head"]["num_classes"] = len(classes)
    cfg["model"]["arch"]["aux_head"]["num_classes"] = len(classes)
    for split in ("train", "val"):
        d = cfg["data"][split]
        d["img_path"], d["ann_path"], d["input_size"] = f"{data}/{split}", f"{data}/{split}.json", size
        d["pipeline"] = dict(t.get("augment", {}), normalize=norm) if split == "train" else {"normalize": norm}
    cfg["device"]["workers_per_gpu"] = t["workers"]
    cfg["device"]["batchsize_per_gpu"] = t["batch"]
    s = cfg["schedule"]
    s["optimizer"]["lr"], s["optimizer"]["weight_decay"] = t["lr"], t["weight_decay"]
    s["warmup"]["steps"] = t["warmup_steps"]
    s["total_epochs"] = t["epochs"]
    s["lr_schedule"]["T_max"] = t.get("schedule_epochs", t["epochs"])
    s["val_intervals"] = t["val_interval"]
    return cfg


def cmd_run(recipe: Recipe, a):
    from . import evaluate as ev
    out = recipe.out_dir(a.out)
    steps = a.steps.split(",") if a.steps else STEPS
    nanodet = recipe.path(recipe.get("train.nanodet"))
    cfg_path = f"{out}/nanodet.yml"
    timings, t_all = {}, time.time()
    if "config" in steps:
        with open(cfg_path, "w", encoding="utf-8", newline="\n") as f:
            yaml.safe_dump(config(recipe, out), f, sort_keys=False)
        print(f"config: {cfg_path}", flush=True)
    if "train" in steps:
        t0 = time.time()
        cmd = [sys.executable, "-I", "tools/train.py", cfg_path]
        if recipe.get("train.seed", None) is not None:
            cmd += ["--seed", str(recipe.get("train.seed"))]
        print("train:", " ".join(cmd), f"(in {nanodet})", flush=True)
        subprocess.run(cmd, cwd=nanodet, check=True)
        timings["train"] = round(time.time() - t0, 1)
    if "export" in steps:
        t0 = time.time()
        sel = recipe.get("train.checkpoint", "best")
        ckpt = a.checkpoint or (f"{out}/nanodet/model_best/nanodet_model_best.pth" if sel == "best"
                                else f"{out}/nanodet/model_last.ckpt")
        cmd = [sys.executable, "-I", "tools/export_onnx.py", "--cfg_path", cfg_path, "--model_path", ckpt,
               "--out_path", f"{out}/model.onnx",
               "--input_shape", f"{recipe.get('input.height')},{recipe.get('input.width')}"]
        subprocess.run(cmd, cwd=nanodet, check=True, stdout=subprocess.DEVNULL)
        print(f"export: {ckpt} -> {out}/model.onnx", flush=True)
        timings["export"] = round(time.time() - t0, 1)
    model = a.model or f"{out}/model.onnx"
    if "eval" in steps or "package" in steps:
        # a package folder without weights yet: the manifest is all eval needs besides the ONNX
        stage = f"{out}/package"
        os.makedirs(stage, exist_ok=True)
        with open(f"{stage}/model.toml", "w", encoding="utf-8", newline="\n") as f:
            f.write(pkg.manifest(pkg.head(recipe), sections(recipe, cfg_path)))
    if "eval" in steps:
        threads, thr = recipe.get("eval.threads", 3), recipe.get("eval.iou", 0.5)
        res = ev.evaluate(ev.Package(f"{out}/package", threads, model), recipe.path(recipe.get("data")),
                          recipe.get("eval.split", "val"), thr)
        print(ev.show(res, threads), flush=True)
        with open(f"{out}/eval.json", "w", encoding="utf-8") as f:
            json.dump(res, f, indent=1)
    if "package" in steps:
        with open(f"{out}/eval.json", encoding="utf-8") as f:
            res = json.load(f)
        data = recipe.path(recipe.get("data"))
        sizes = {}
        for split in ("train", "val"):
            c = json.load(open(f"{data}/{split}.json", encoding="utf-8"))
            sizes[split] = {"images": len(c["images"]), "annotations": len(c["annotations"]),
                            "without_labels": len({i["id"] for i in c["images"]} - {x["image_id"] for x in c["annotations"]})}
        d = pkg.write(f"{out}/package", recipe, sections(recipe, cfg_path), {"model.onnx": model},
                      {"eval": res, "data": sizes},
                      ev.markdown(res, recipe.get("eval.iou", 0.5), recipe.get("eval.threads", 3)))
        print(f"package: {d}", flush=True)
    print(f"timings (s): {json.dumps(timings)}, total {time.time() - t_all:.0f} s", flush=True)


def sections(recipe: Recipe, cfg_path: str) -> dict:
    with open(cfg_path, encoding="utf-8") as f:
        head = yaml.safe_load(f)["model"]["arch"]["head"]
    mean, std = recipe.get("normalize.mean"), recipe.get("normalize.std")
    return {"input": {"order": "bgr", "mean": [round(m / 255, 6) for m in mean], "std": [round(s / 255, 6) for s in std],
                      "width": recipe.get("input.width"), "height": recipe.get("input.height")},
            "nanodet": {"strides": head["strides"], "reg_max": head["reg_max"], "classes": recipe.get("classes"),
                        "score": recipe.get("decode.score"), "nms": recipe.get("decode.nms")}}


def cmd_eval(recipe, a):
    from . import evaluate as ev
    threads, thr = recipe.get("eval.threads", 3), recipe.get("eval.iou", 0.5)
    res = ev.evaluate(ev.Package(a.package, threads), recipe.path(recipe.get("data")), a.split, thr)
    print(ev.show(res, threads))
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump(res, f, indent=1)
    print(ev.markdown(res, thr, threads))


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv and argv[0] not in COMMANDS and not argv[0].startswith("-"):
        argv.insert(0, "run")
    ap = argparse.ArgumentParser(prog="training/layout", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("run")
    add_recipe_args(p)
    p.add_argument("--steps", help="comma-separated subset of " + ",".join(STEPS))
    p.add_argument("--checkpoint", help="export this checkpoint instead of the trained one")
    p.add_argument("--model", help="evaluate and package this ONNX instead of <out>/model.onnx")
    p = sub.add_parser("eval")
    add_recipe_args(p)
    p.add_argument("--package", required=True)
    p.add_argument("--split", default="val")
    p.add_argument("--json")
    p = sub.add_parser("labels")
    add_recipe_args(p)
    p.add_argument("--split", default="val")
    p.add_argument("--n", type=int, default=6)
    p.add_argument("--seed", type=int, default=1)
    p = sub.add_parser("errors")
    add_recipe_args(p)
    p.add_argument("--package", required=True)
    p.add_argument("--split", default="val")
    p.add_argument("--n", type=int, default=6)
    p = sub.add_parser("shots")
    add_recipe_args(p)
    p.add_argument("--package", required=True)
    p.add_argument("images", nargs="+")
    a = ap.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8")
    recipe = Recipe(a.recipe, a.data_root, a.set)
    if recipe.family != "nanodet":
        ap.error(f"{a.recipe}: family {recipe.family}, this pipeline makes nanodet")
    if a.cmd == "run":
        cmd_run(recipe, a)
    elif a.cmd == "eval":
        cmd_eval(recipe, a)
    else:
        from . import sheets as ins
        out = recipe.out_dir(a.out)
        if a.cmd == "labels":
            ins.labels(recipe.path(recipe.get("data")), a.split, f"{out}/labels-{a.split}.png", a.n, a.seed)
        elif a.cmd == "errors":
            ins.errors(a.package, recipe.path(recipe.get("data")), a.split, f"{out}/errors-{a.split}.png", a.n,
                       recipe.get("eval.iou", 0.5))
        else:
            ins.shots(a.package, a.images, f"{out}/shots.png", recipe.get("shots"))


if __name__ == "__main__":
    main()
