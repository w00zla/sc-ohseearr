"""Real warehouse and refinery lines labelled by a teacher model -> line datasets (against forgetting in the
recognizer's fine-tune: sc-ohseearr's recipe sc-rec-latin trains on them alongside its synthetic lines).

Input: the box dumps the examples wrote with the teacher (the recognizer cut to Latin, before the fine-tune):
`<teacher>/inv-<rec>/boxes-<frame>.txt` (the Quartermaster companion's `inventory_dump` over a warehouse
recording) and `<teacher>/ref/<folder>__<screenshot>.txt` (`ocr_dump` over refinery screenshots; spaces in the
folder name as "_"). Every box line `x y w h score text` with score >= 0.95 and at least two characters becomes a
crop. Train: warehouse recording ...6676 and the refinery screenshots except the held-out frames (GATE);
eval: warehouse recording ...6994 and the held-out frames.
Output: `<root>/ocr-train/teacher/labels.tsv` (file, text; no header) and `<root>/ocr-eval-teacher/labels.tsv`
(header: file, set, source, label) with their crops.

  PY training/tools/teacher_lines.py [--data-root <dir>] [--teacher <root>/ocr-teacher]
PY = local/detector/.venv/Scripts/python.exe -I (from the repository root). The data root: `--data-root`, else
$SC_OHSEEARR_DATA, else <repo>/local (frames in its sources/: the recordings as qm-inventory-debug-<rec>/, the
refinery screenshots under their folder names).
"""
import argparse
import os
import re
import sys

import cv2

GATE = {"20D", "E87", "0F4", "DC5", "2E4", "BB4", "575"}     # suffixes of the refinery frames held out for the eval
MIN_SCORE = 0.95
TRAIN_REC, EVAL_REC = "1793396676", "1793396994"
LINE = re.compile(r"^\s*(-?\d+)\s+(-?\d+)\s+(\d+)\s+(\d+)\s+([\d.]+)\s+(.+)$")


def boxes(path):
    for line in open(path, encoding="utf-8"):
        m = LINE.match(line.rstrip("\n"))
        if m and not m[6].startswith("[") and float(m[5]) >= MIN_SCORE and len(m[6].strip()) >= 2:
            yield int(m[1]), int(m[2]), int(m[3]), int(m[4]), m[6].strip()


def cut(img, x, y, w, h):
    H, W = img.shape[:2]
    x0, y0 = max(0, x), max(0, y)
    return img[y0:min(H, y + h + 1), x0:min(W, x + w + 1)]


def write(out_dir, items, header):
    os.makedirs(out_dir, exist_ok=True)
    rows = []
    for k, (img_path, box, set_name) in enumerate(items):
        img = cv2.imread(img_path)
        crop = cut(img, *box[:4])
        if crop.size == 0 or crop.shape[0] < 6:
            continue
        name = f"{k:06d}.png"
        cv2.imwrite(os.path.join(out_dir, name), crop)
        rows.append((name, set_name, box[4]))
    with open(os.path.join(out_dir, "labels.tsv"), "w", encoding="utf-8", newline="\n") as f:
        if header:
            f.write("file\tset\tsource\tlabel\n")
            f.writelines(f"{n}\t{s}\tteacher\t{t}\n" for n, s, t in rows)
        else:
            f.writelines(f"{n}\t{t}\n" for n, _, t in rows)
    return len(rows)


def warehouse(local, teacher, rec_id, folder):
    for name in sorted(os.listdir(f"{teacher}/{folder}")):
        if name.startswith("boxes-"):
            img = f"{local}/sources/qm-inventory-debug-{rec_id}/{name[6:-4]}.jpg"
            for b in boxes(f"{teacher}/{folder}/{name}"):
                yield img, b, f"wh-{folder[-4:]}"


def refinery(local, teacher, gate: bool):
    for name in sorted(os.listdir(f"{teacher}/ref")):
        d, stem = name[:-4].split("__")
        if (stem[-3:] in GATE) != gate:
            continue
        folder = f"{local}/sources/{d.replace('_', ' ')}"
        img = next(f"{folder}/{f}" for f in os.listdir(folder) if os.path.splitext(f)[0] == stem)
        for b in boxes(f"{teacher}/ref/{name}"):
            yield img, b, "refinery"


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    ap = argparse.ArgumentParser()
    ap.add_argument("--data-root", default=os.environ.get("SC_OHSEEARR_DATA") or os.path.join(repo, "local"))
    ap.add_argument("--teacher", help="the teacher's dumps (default <root>/ocr-teacher)")
    ap.add_argument("--train-out", help="default <root>/ocr-train/teacher")
    ap.add_argument("--eval-out", help="default <root>/ocr-eval-teacher")
    a = ap.parse_args()
    local = a.data_root.replace("\\", "/")
    teacher = a.teacher or f"{local}/ocr-teacher"
    n_train = write(a.train_out or f"{local}/ocr-train/teacher",
                    list(warehouse(local, teacher, TRAIN_REC, f"inv-{TRAIN_REC[-4:]}")) + list(refinery(local, teacher, False)),
                    header=False)
    n_eval = write(a.eval_out or f"{local}/ocr-eval-teacher",
                   list(warehouse(local, teacher, EVAL_REC, f"inv-{EVAL_REC[-4:]}")) + list(refinery(local, teacher, True)),
                   header=True)
    print(f"teacher train {n_train} lines, eval {n_eval} lines")


if __name__ == "__main__":
    main()
