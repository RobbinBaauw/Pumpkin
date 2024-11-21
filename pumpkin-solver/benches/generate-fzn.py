import itertools
import multiprocessing
import shutil
import subprocess
from collections import defaultdict
from multiprocessing.pool import ThreadPool
from pathlib import Path
from typing import Optional

BENCH_DIR = Path(__file__).parent / "minizinc-examples"
BENCH_SET_DIR = Path(__file__).parent / "examples-set"

OUT_EXT = "lp"

PUMPKIN_SOLVER = Path(__file__).parent.parent.parent / "minizinc" / "pumpkin-linear-ineq.msc"

TOTAL_SAMPLES = 500

thread_pool = ThreadPool()


def find_fzn_name_path(mzn_path: Path, dzn_path: Optional[Path]):
    if dzn_path is None:
        fzn_name = f"{mzn_path.stem}.{OUT_EXT}"
        fzn_path = mzn_path.parent / fzn_name
    else:
        dzn_name = dzn_path.stem

        dzn_parent_path = dzn_path.parent
        while mzn_path.parent != dzn_parent_path:
            # dzn in subdir
            dzn_name = f"{dzn_parent_path.stem}_{dzn_name}"
            dzn_parent_path = dzn_parent_path.parent

        fzn_name = f"{mzn_path.stem}_{dzn_name}.{OUT_EXT}"
        fzn_path = mzn_path.parent / fzn_name

    return fzn_name, fzn_path


def worker_generate(mzn_path: Path, dzn_path: Optional[Path]):
    fzn_name, fzn_path = find_fzn_name_path(mzn_path, dzn_path)

    if fzn_path.exists():
        print(f"({fzn_name}) Already exists, skip!")
        return

    print(f"({multiprocessing.current_process().name}) Generating {fzn_path}")

    match OUT_EXT:
        case "fzn":
            cmd = ["minizinc", "-c", str(mzn_path.resolve()),
                    None if dzn_path is None else str(dzn_path.resolve()), "--solver", str(PUMPKIN_SOLVER.resolve()),
                    "--fzn", str(fzn_path.resolve())]
        case "lp":
            cmd = ["minizinc", "--solver", "cplex",
                  "--writeModel", str(fzn_path.resolve()),
                  "--cplex-dll", "/opt/ibm/ILOG/CPLEX_Studio2211/cplex/bin/x86-64_linux/libcplex2211.so",
                  "--solver-time-limit", "1",
                  str(mzn_path.resolve()), None if dzn_path is None else str(dzn_path.resolve())]
        case _:
            raise RuntimeError("Invalid file format")

    cmd = list(filter(lambda c: c is not None, cmd))

    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    try:
        stdout, stderr = p.communicate(timeout=10)
        if p.returncode != 0:
            print(f"({fzn_name}) Error! {stderr}")
        else:
            print(f"({fzn_name}) Done, next!")
    except subprocess.TimeoutExpired as e:
        p.kill()
        print(f"({fzn_name}) Timeout, skip!")


def search_dir(dir_path: Path):
    mzn_files = list(dir_path.rglob("./*.mzn"))

    dzn_files = list(dir_path.rglob("./*.dzn"))
    if len(mzn_files) > 0 and len(dzn_files) == 0:
        dzn_files = [None]

    model_data_combos = itertools.product(mzn_files, dzn_files)
    for (model, data) in model_data_combos:
        thread_pool.apply_async(worker_generate, (model, data))


def generate_fzn():
    for f in BENCH_DIR.iterdir():
        if f.is_dir():
            search_dir(f)

    thread_pool.close()
    thread_pool.join()


def select_fzn():
    BENCH_SET_DIR.mkdir(exist_ok=True)

    mzn_fzn_files = []

    for f in BENCH_DIR.iterdir():
        if f.is_dir():
            mzn_fzn_files.append((f, list(f.rglob(f"./*.{OUT_EXT}"))))

    mzn_fzn_files = list(filter(lambda i: len(i[1]) != 0, mzn_fzn_files))

    mzn_fzn_paths = defaultdict(list)
    bench_idx = defaultdict(lambda: 0)

    def add_next_file(mzn, fzns):
        while True:
            fzn_idx = bench_idx[mzn_idx]

            if fzn_idx >= len(fzns):
                return
            else:
                bench_idx[mzn_idx] += 1
                mzn_fzn_paths[mzn.name].append(fzns[fzn_idx])
                return

    prev_size = 0
    while get_total_values_size(mzn_fzn_paths) < TOTAL_SAMPLES:
        for mzn_idx, (mzn, fzns) in enumerate(mzn_fzn_files):
            add_next_file(mzn, fzns)

        new_size = get_total_values_size(mzn_fzn_paths)
        if prev_size == new_size:
            break
        prev_size = new_size

    for (mzn_dir, fzn_paths) in mzn_fzn_paths.items():
        new_dir = BENCH_SET_DIR / mzn_dir
        new_dir.mkdir(exist_ok=True)

        for path in fzn_paths:
            new_path = new_dir / path.name
            if new_path.exists():
                print(f"Skipping copy of {new_path.name}")
                continue

            print(f"Copying {new_path.name}")
            shutil.copy(path, new_path)


def get_total_values_size(mzn_fzn_paths):
    return sum(map(lambda v: len(v), mzn_fzn_paths.values()))


if __name__ == "__main__":
    # generate_fzn()
    select_fzn()