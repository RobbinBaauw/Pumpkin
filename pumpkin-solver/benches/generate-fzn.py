import itertools
import multiprocessing
import subprocess
from multiprocessing.pool import ThreadPool
from pathlib import Path

BENCH_DIR = Path(__file__).parent / "minizinc-benchmarks"
PUMPKIN_SOLVER = Path(__file__).parent.parent.parent / "minizinc" / "pumpkin-linear-ineq.msc"

thread_pool = ThreadPool()


def worker_generate(mzn_path: Path, dzn_path: Path):
    dzn_name = dzn_path.stem

    dzn_parent_path = dzn_path.parent
    while mzn_path.parent != dzn_parent_path:
        # dzn in subdir
        dzn_name = f"{dzn_parent_path.stem}_{dzn_name}"
        dzn_parent_path = dzn_parent_path.parent

    fzn_name = f"{mzn_path.stem}_{dzn_name}.fzn"
    fzn_path = mzn_path.parent / fzn_name

    if fzn_path.exists():
        print(f"({multiprocessing.current_process().name}) Already exists, skip!")
        return

    print(f"({multiprocessing.current_process().name}) Generating {fzn_path}")
    cmd = ["minizinc", "-c", str(mzn_path.resolve()),
            str(dzn_path.resolve()), "--solver", str(PUMPKIN_SOLVER.resolve()),
            "--fzn", str(fzn_path.resolve())]

    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    try:
        p.communicate(timeout=10)
        print(f"({multiprocessing.current_process().name}) Done, next!")
    except subprocess.TimeoutExpired as e:
        p.kill()
        print(f"({multiprocessing.current_process().name}) Timeout, skip!")


def search_dir(dir_path: Path):
    mzn_files = list(dir_path.rglob("./*.mzn"))
    dzn_files = list(dir_path.rglob("./*.dzn"))

    model_data_combos = itertools.product(mzn_files, dzn_files)
    for (model, data) in model_data_combos:
        thread_pool.apply_async(worker_generate, (model, data))


if __name__ == "__main__":
    for f in BENCH_DIR.iterdir():
        if f.is_dir():
            search_dir(f)

    thread_pool.close()
    thread_pool.join()

