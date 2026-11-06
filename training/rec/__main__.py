"""The recognizer pipeline (family `ctc-rec`): recipe -> model package.

  PY training/rec <recipe> [--steps cut,backgrounds,synth,train,export,eval,package] [--out <dir>]
  PY training/rec eval <recipe> --model <model.onnx> [--dict <dict.txt>] [--sets a,b]
  PY training/rec teacher <recipe> --src <crops> --dest <dir> [--model <onnx>] [--min-score 0.95]
  PY training/rec backgrounds <recipe> [--force]
  PY training/rec sheet <recipe> [--real <lines dir>]       (synthetic vs real lines, for judging the renderer)

PY = local/detector/.venv/Scripts/python.exe -I (from the repository root). Steps of a run:
  cut          the original recognizer's head cut to the charset: <out>/cut/{dict.txt, model.onnx}
  backgrounds  builds missing background banks that have a `build` section
  synth        renders <out>/synth and <out>/synth-val (skipped when their labels.tsv exists)
  train        fine-tunes from the original weights: <out>/train/{last,best}.pt, log.json
  export       <out>/model.onnx
  eval         the recipe's eval sets on <out>/model.onnx (and the cut model as the baseline)
  package      <out>/package: model.onnx, model.toml, dict.txt, metrics.json, MODEL.metrics.md
"""
import argparse
import json
import os
import sys
import time

if __package__ in (None, ""):                       # run as `python training/rec`: import as a package
    _root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    sys.path = [p for p in sys.path if os.path.abspath(p) != os.path.dirname(os.path.abspath(__file__))]
    sys.path.insert(0, _root)
    __package__ = "training.rec"

from ..common import package as pkg  # noqa: E402
from ..common.recipe import Recipe, add_recipe_args  # noqa: E402

STEPS = ["cut", "backgrounds", "synth", "train", "export", "eval", "package"]
COMMANDS = ["run", "eval", "teacher", "backgrounds", "sheet"]


def input_of(recipe):
    from .model import Input
    i = recipe.get("input")
    return Input(i["height"], i["width"], i.get("max_width", 1920))


def cmd_run(recipe: Recipe, a):
    from . import cut, evaluate as ev, model as M
    out = recipe.out_dir(a.out)
    steps = a.steps.split(",") if a.steps else STEPS
    inp = input_of(recipe)
    timings = {}
    t_all = time.time()

    def step(name):
        if name in steps:
            timings[name] = time.time()
            print(f"== {name}", flush=True)
            return True
        return False

    def done(name):
        timings[name] = round(time.time() - timings[name], 1)

    dict_path, cut_model = f"{out}/cut/dict.txt", f"{out}/cut/model.onnx"
    if step("cut"):
        cut.run(recipe, out)
        done("cut")
    charset = M.Charset.load(dict_path)
    synth_cfg = recipe.get("synth", None)
    if step("backgrounds") and synth_cfg:
        from . import backgrounds
        backgrounds.build(recipe)
        done("backgrounds")
    if step("synth") and synth_cfg:
        from . import render
        workers = synth_cfg.get("workers") or max(1, (os.cpu_count() or 2) - 2)
        cs = set(charset.chars) | {" "}
        for sub, n, seed in (("synth", synth_cfg["count"], synth_cfg["seed"]),
                             ("synth-val", synth_cfg["val_count"], synth_cfg["val_seed"])):
            if os.path.exists(f"{out}/{sub}/labels.tsv"):
                print(f"{sub}: exists, kept", flush=True)
                continue
            render.generate(synth_cfg, recipe, cs, n, f"{out}/{sub}", seed, workers)
        done("synth")
    if step("train"):
        from . import train
        b = recipe.get("base.weights")
        _, keep = cut.plan(recipe)
        model = M.load(b["repo"], b["revision"], keep)
        resolve = lambda p: f"{out}/synth" if p == "@synth" else f"{out}/synth-val" if p == "@synth-val" else recipe.path(p)
        data = [(resolve(d["path"]), d.get("repeat", 1)) for d in recipe.get("train.data")]
        normalize = recipe.get("eval.normalize", {})
        monitor = []
        for m in recipe.get("train.monitor", []):
            spec = recipe.get("eval.sets").get(m["set"], {"path": m["set"]})
            rows = ev.rows_of(resolve(spec["path"]), charset, normalize, m.get("limit"), m.get("seed", 0))
            monitor.append((m["set"], rows, spec))
        ckpt = train.train(recipe, model, charset, inp, data, monitor, f"{out}/train")
        with open(f"{out}/train/selected.txt", "w") as f:
            f.write(os.path.basename(ckpt))
        done("train")
    if step("export"):
        from . import export
        b = recipe.get("base.weights")
        _, keep = cut.plan(recipe)
        sel = a.checkpoint or f"{out}/train/" + open(f"{out}/train/selected.txt").read().strip()
        export.export(M.load(b["repo"], b["revision"], keep, sel), f"{out}/model.onnx", inp.height)
        done("export")
    model_path = a.model or f"{out}/model.onnx"
    if step("eval"):
        res = ev.evaluate(recipe, ev.onnx_reader(model_path, charset, inp), charset, inp)
        base = None
        if recipe.get("eval.baseline", False):
            print("baseline (cut model):", flush=True)
            base = ev.evaluate(recipe, ev.onnx_reader(cut_model, charset, inp), charset, inp)
        with open(f"{out}/eval.json", "w", encoding="utf-8") as f:
            json.dump({"model": res, "baseline": base}, f, indent=1)
        done("eval")
    if step("package"):
        with open(f"{out}/eval.json", encoding="utf-8") as f:
            e = json.load(f)
        i = recipe.get("input")
        sections = {"input": {"order": "bgr", "mean": [0.5, 0.5, 0.5], "std": [0.5, 0.5, 0.5],
                              "height": i["height"], "width": i["width"]},
                    "ctc": {"dict": "dict.txt", "space_char": True}}
        metrics = {"eval": e["model"], "baseline": e["baseline"], "data": data_sizes(recipe, out)}
        d = pkg.write(f"{out}/package", recipe, sections, {"model.onnx": model_path, "dict.txt": dict_path}, metrics,
                      ev.markdown(recipe, e["model"], e["baseline"]))
        print(f"package: {d}", flush=True)
        done("package")
    print(f"timings (s): {json.dumps(timings)}, total {time.time() - t_all:.0f} s", flush=True)


def data_sizes(recipe, out):
    from ..common.datasets import read_lines
    sizes = {}
    for d in recipe.get("train.data", []):
        p = f"{out}/synth" if d["path"] == "@synth" else recipe.path(d["path"])
        if os.path.exists(f"{p}/labels.tsv"):
            sizes[d["path"]] = {"lines": len(read_lines(p)), "repeat": d.get("repeat", 1)}
    return sizes


def cmd_eval(recipe, a):
    from . import evaluate as ev, model as M
    charset = M.Charset.load(a.dict or f"{os.path.dirname(os.path.abspath(a.model))}/dict.txt")
    inp = input_of(recipe)
    t0 = time.time()
    res = ev.evaluate(recipe, ev.onnx_reader(a.model, charset, inp), charset, inp, a.sets.split(",") if a.sets else None)
    print(f"({time.time() - t0:.0f} s)")
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump(res, f, indent=1)
    print(ev.markdown(recipe, res, None))


def cmd_teacher(recipe, a):
    from . import teacher
    out = recipe.out_dir(a.out)
    model = a.model or f"{out}/cut/model.onnx"
    teacher.label(model, a.dict or f"{os.path.dirname(os.path.abspath(model))}/dict.txt", input_of(recipe),
                  a.src, a.dest, a.min_score)


def cmd_backgrounds(recipe, a):
    from . import backgrounds
    backgrounds.build(recipe, force=a.force)


def cmd_sheet(recipe, a):
    from . import sheet
    out = recipe.out_dir(a.out)
    sheet.sheet(f"{out}/sheet.png", f"{out}/synth", recipe.path(a.real) if a.real else None)


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv and argv[0] not in COMMANDS and not argv[0].startswith("-"):
        argv.insert(0, "run")
    ap = argparse.ArgumentParser(prog="training/rec", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("run")
    add_recipe_args(p)
    p.add_argument("--steps", help="comma-separated subset of " + ",".join(STEPS))
    p.add_argument("--checkpoint", help="export this state_dict instead of the trained one")
    p.add_argument("--model", help="evaluate and package this ONNX instead of <out>/model.onnx")
    p = sub.add_parser("eval")
    add_recipe_args(p)
    p.add_argument("--model", required=True)
    p.add_argument("--dict", help="the model's dictionary (default: dict.txt next to the model)")
    p.add_argument("--sets", help="comma-separated eval sets (default: all)")
    p.add_argument("--json", help="write the metrics here")
    p = sub.add_parser("teacher")
    add_recipe_args(p)
    p.add_argument("--src", required=True)
    p.add_argument("--dest", required=True)
    p.add_argument("--model")
    p.add_argument("--dict")
    p.add_argument("--min-score", type=float, default=0.95)
    p = sub.add_parser("backgrounds")
    add_recipe_args(p)
    p.add_argument("--force", action="store_true")
    p = sub.add_parser("sheet")
    add_recipe_args(p)
    p.add_argument("--real")
    a = ap.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8")
    recipe = Recipe(a.recipe, a.data_root, a.set)
    if recipe.family != "ctc-rec":
        ap.error(f"{a.recipe}: family {recipe.family}, this pipeline makes ctc-rec")
    {"run": cmd_run, "eval": cmd_eval, "teacher": cmd_teacher, "backgrounds": cmd_backgrounds, "sheet": cmd_sheet}[a.cmd](recipe, a)


if __name__ == "__main__":
    main()
