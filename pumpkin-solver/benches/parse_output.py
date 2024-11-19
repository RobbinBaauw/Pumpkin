import json
import re
import sys
from collections import defaultdict
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Dict, Optional, List
import pickle

from jinja2 import Environment, PackageLoader, select_autoescape, FileSystemLoader
from pip._vendor import tomli
import jinja2

BASE_DIR = Path(__file__).parent / "results" / "experiments"


@dataclass
class LinearLeq:
    is_learned: bool
    num_executions: int
    num_propagations: int


class Result(Enum):
    UNSAT = "unsat"
    UNKNOWN = "unknown"
    SUCCESS = "success"


@dataclass
class RunData:
    bench_version: str

    fzn_file_name: str

    result: Result
    result_values: Optional[str]

    use_intsat: bool
    skip_nogood_learning: bool

    num_decisions: int
    num_conflicts: int
    num_restarts: int
    num_propagations: int

    intsat_learned_constraints: int
    intsat_learned_constraints_avg_length: float
    intsat_learned_constraints_avg_coeff: float
    intsat_fallback_used: int

    linear_leqs: Dict[int, LinearLeq]


@dataclass
class RunError:
    stderr: str


@dataclass
class RunResult:
    exit_code: int
    wall_time: int

    run_error: Optional[RunError]
    run_data: Optional[RunData]


Results = Dict[str, List[Optional[RunResult]]]

def parse_metrics(metrics_path: Path):
    with open(metrics_path, 'rb') as metrics_file:
        metrics = tomli.load(metrics_file)

    exit_code = metrics['exit_code']
    wall_time = metrics['wall_micros']['secs'] + metrics['wall_micros']['nanos'] / 1e9

    return exit_code, wall_time


def parse_stderr(stderr_path: Path):
    with open(stderr_path) as stderr_file:
        stderr = stderr_file.read()

    return RunError(stderr) if len(stderr) > 0 else None


def parse_stat_line(stat_line: str):
    stat_res = re.search("^\$stat\$-I(.+)-SL(.+) (.+)=(.+)$", stat_line)
    if stat_res is None:
        raise RuntimeError(f"Cannot parse line {stat_line}")

    return bool(stat_res.group(1)), bool(stat_res.group(2)), stat_res.group(3), json.loads(stat_res.group(4))


def parse_stdout(stdout_path: Path):
    with open(stdout_path) as stdout_file:
        stdout = stdout_file.read().split("\n")

    # First version of experiments still had the arguments logged
    if stdout[0].startswith("--"):
        stdout = stdout[1:]

    version = stdout[0]
    if not version.startswith('V'): raise RuntimeError(f"Invalid version in stdout {stdout}")

    file_name_match = re.search("^Executing \"(.+)\"", stdout[1])
    if file_name_match is None: raise RuntimeError(f"Invalid file name in stdout {stdout}")
    file_name = file_name_match.group(1)

    # If we only have 2 full lines, we're not done solving
    if len(stdout) <= 2 or len(stdout[2]) == 0:
        return None

    result_line_i = 2
    if "=====UNKNOWN=====" in stdout[result_line_i]:
        result = Result.UNKNOWN
        result_values = None
    elif "=====UNSATISFIABLE=====" in stdout[result_line_i]:
        result = Result.UNSAT
        result_values = None
    else:
        result = Result.SUCCESS

        # TODO make sure they always have a result
        if stdout[result_line_i].startswith("$stat$"):
            result_values = None
        else:
            result_lines = []
            while "----------" not in stdout[result_line_i]:
                result_lines.append(stdout[result_line_i])
                result_line_i += 1

            result_values = "\n".join(result_lines)

    # Not sure where that comes from
    if "==========" in stdout[result_line_i + 1]:
        result_line_i += 1

    use_intsat, skip_nogood_learning, _, _ = parse_stat_line(stdout[result_line_i + 1])

    linear_leq_id_values = defaultdict(lambda: LinearLeq(False, 0, 0))
    for line_i in range(result_line_i + 1, len(stdout)):
        line_val = stdout[line_i]
        if len(line_val) == 0:
            continue

        _, _, stat, value = parse_stat_line(line_val)

        match stat:
            case "_engine_statistics_num_decisions":
                num_decisions = value
            case "_engine_statistics_num_conflicts":
                num_conflicts = value
            case "_engine_statistics_num_restarts":
                num_restarts = value
            case "_engine_statistics_num_propagations":
                num_propagations = value
            case "_intsat_statistics_intsat_learned_constraints":
                intsat_learned_constraints = value
            case "_intsat_statistics_intsat_learned_constraints_avg_length":
                intsat_learned_constraints_avg_length = value
            case "_intsat_statistics_intsat_constraint_avg_lhs_coeff":
                intsat_learned_constraints_avg_coeff = value
            case "_intsat_statistics_intsat_fallback_used":
                intsat_fallback_used = value

        linear_leq_res = re.search("^LinearLeq_number_(\d+)_(.+)$", stat)
        if linear_leq_res is not None:
            linear_leq_id, linear_leq_field = int(linear_leq_res.group(1)), linear_leq_res.group(2)

            match linear_leq_field:
                case "is_learned":
                    linear_leq_id_values[linear_leq_id].is_learned = value
                case "number_of_executions":
                    linear_leq_id_values[linear_leq_id].num_executions = value
                case "number_of_propagations":
                    linear_leq_id_values[linear_leq_id].num_propagations = value

    return RunData(version,
                   file_name,
                   result,
                   result_values,
                   use_intsat,
                   skip_nogood_learning,
                   num_decisions,
                   num_conflicts,
                   num_restarts,
                   num_propagations,
                   intsat_learned_constraints,
                   intsat_learned_constraints_avg_length,
                   intsat_learned_constraints_avg_coeff,
                   intsat_fallback_used,
                   dict(linear_leq_id_values))


def parse_results_dir(results_dir: Path):
    results = []
    for exp_dir in results_dir.iterdir():
        if exp_dir.is_dir():
            # Skip unfinished runs
            metrics_content = (exp_dir / "metrics").open().read()
            if len(metrics_content) == 0 or "NotCompleted" in metrics_content:
                print(f"Skipping {exp_dir}, still empty")
                continue

            print(f"Parsing {exp_dir}")
            exit_code, wall_time = parse_metrics(exp_dir / "metrics")
            run_err = parse_stderr(exp_dir / "stderr")
            run_data = parse_stdout(exp_dir / "stdout")
            results.append(RunResult(exit_code, wall_time, run_err, run_data))

    return results


def solved_problem_names(*runs: List[RunResult]):
    problems = set()

    for run in runs:
        for res in run:
            if res.run_data is not None:
                problems.add(res.run_data.fzn_file_name)

    return list(problems)



def update_results(results: Results, prog_results: List[RunResult], prog_idx: int):
    for prog_res in prog_results:
        if prog_res.run_data is not None:
            results[prog_res.run_data.fzn_file_name][prog_idx] = prog_res


def parse_results():
    resolution_results = parse_results_dir(BASE_DIR / "1" / "2")
    intsat_skip_results = parse_results_dir(BASE_DIR / "3" / "0")
    intsat_results = parse_results_dir(BASE_DIR / "3" / "1")

    results = {
        prob: [None, None, None]
        for prob in solved_problem_names(resolution_results, intsat_skip_results, intsat_results)
    }

    update_results(results, resolution_results, 0)
    update_results(results, intsat_skip_results, 1)
    update_results(results, intsat_results, 2)

    with open('results_out.pkl', 'wb') as results_out:
        pickle.dump(results, results_out, pickle.HIGHEST_PROTOCOL)


def did_fail(res: Optional[RunResult]):
    return ((res is None) or
            (res.exit_code != 0) or
            (res.wall_time > 3600) or
            (res.run_data is None) or
            (res.run_data.result == Result.UNKNOWN))


def results_to_table(results: Results):
    table = []

    for prob in results.keys():
        resolution, intsat_skip, intsat = results[prob]

        if not did_fail(resolution):
            res_conflicts = resolution.run_data.num_conflicts
        else:
            res_conflicts = None

        if not did_fail(intsat_skip):
            intsat_skip_conflicts = intsat_skip.run_data.num_conflicts
            intsat_skip_constraints = intsat_skip.run_data.num_conflicts
            intsat_skip_fallbacks = intsat_skip.run_data.intsat_fallback_used
            intsat_skip_learned_propagations = sum(map(lambda leq: leq.num_propagations if leq.is_learned else 0, intsat_skip.run_data.linear_leqs.values()))
        else:
            intsat_skip_conflicts = intsat_skip_constraints = intsat_skip_fallbacks = intsat_skip_learned_propagations = None

        if not did_fail(intsat):
            intsat_conflicts = intsat.run_data.num_conflicts
            intsat_constraints = intsat.run_data.num_conflicts
            intsat_fallbacks = intsat.run_data.intsat_fallback_used
            intsat_learned_propagations = sum(map(lambda leq: leq.num_propagations if leq.is_learned else 0, intsat.run_data.linear_leqs.values()))
        else:
            intsat_conflicts = intsat_constraints = intsat_fallbacks = intsat_learned_propagations = None

        if did_fail(resolution) and did_fail(intsat_skip) and did_fail(intsat):
            results.pop(prob)
            continue

        best_conf = min(
            res_conflicts if res_conflicts is not None else sys.maxsize,
            intsat_skip_conflicts if intsat_skip_conflicts is not None else sys.maxsize,
            intsat_conflicts if intsat_conflicts is not None else sys.maxsize)

        table.append([prob, (res_conflicts, res_conflicts == best_conf),
                      (intsat_skip_conflicts, intsat_skip_conflicts == best_conf), intsat_skip_constraints, intsat_skip_fallbacks, intsat_skip_learned_propagations,
                      (intsat_conflicts, intsat_conflicts == best_conf), intsat_constraints, intsat_fallbacks, intsat_learned_propagations])

    table_to_latex(table)


def table_to_latex(table):
    env = Environment(
        loader=FileSystemLoader("./"),
        autoescape=select_autoescape()
    )
    env.tests['tuple'] = lambda v: type(v) is tuple
    template = env.get_template("table_out.j2")

    headers = {
        "": ["Problem"],
        "Resolution": ["\# conf"],
        "IntSat skip learning": ["\# conf", "\# lear constr", "\# fallb", "\# learn prop"],
        "IntSat": ["\# conf", "\# lear constr", "\# fallb", "\# learn prop"],
    }
    rendered = template.render({
        "headers": headers,
        "rows": table
    })

    print(rendered)



if __name__ == "__main__":
    # parse_results()

    with open('results_out.pkl', 'rb') as results:
        results_to_table(pickle.load(results))
