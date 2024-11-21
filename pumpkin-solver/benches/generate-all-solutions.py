import subprocess
from multiprocessing.pool import ThreadPool
from pathlib import Path

BENCH_SET_DIR = Path(__file__).parent / "examples-set"
BENCH_SOLUTIONS_DIR = Path(__file__).parent / "examples-solutions"

thread_pool = ThreadPool()


def generate_solutions(fzn_path: Path):
    parent_dir = BENCH_SOLUTIONS_DIR / fzn_path.parent.name
    parent_dir.mkdir(exist_ok=True)

    output_file = parent_dir / fzn_path.name
    output_file = output_file.with_suffix(".ozn")

    if output_file.exists():
        print(f"({fzn_path.name}) Skipping, exists!")
        return

    print(f"({fzn_path.name}) Started generating solutions")

    cmd = ["minizinc", "--solver", "cp-sat",
           "-a", "--output-to-file", str(output_file.resolve()),
           str(fzn_path.resolve())]

    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    try:
        stdout, stderr = p.communicate(timeout=150)
        if p.returncode != 0:
            print(f"({fzn_path.name}) Error! {stderr}")
        else:
            print(f"({fzn_path.name}) Done, next!")
    except subprocess.TimeoutExpired as e:
        p.kill()
        print(f"({fzn_path.name}) Timeout, skip!")


if __name__ == "__main__":
    BENCH_SOLUTIONS_DIR.mkdir(exist_ok=True)

    for f in BENCH_SET_DIR.rglob("*.fzn"):
        thread_pool.apply_async(generate_solutions, (f,))

    thread_pool.close()
    thread_pool.join()

