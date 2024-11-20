import json
import re
import subprocess
from collections import defaultdict
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Dict, Optional, List
import pickle

from jinja2 import Environment, select_autoescape, FileSystemLoader
from pip._vendor import tomli

BENCH_DIR = Path(__file__).parent / "examples-set"
BASE_DIR = Path(__file__).parent / "results" / "experiments"


@dataclass
class LinearLeq:
    num_executions: int
    num_propagations: int


class Result(Enum):
    UNSAT = "unsat"
    UNKNOWN = "unknown"
    SUCCESS = "success"


@dataclass
class RunData:
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

    bench_version: str
    fzn_file_name: str

    run_error: Optional[RunError]
    run_data: Optional[RunData]

    def short_result(self):
        if self.run_error is not None:
            return "E"

        if self.run_data is None:
            return "?"

        if self.wall_time > 3600 or self.run_data.result == Result.UNKNOWN:
            return "T"

        if self.run_data.result == Result.SUCCESS:
            return "S"

        if self.run_data.result == Result.UNSAT:
            return "U"


Results = Dict[str, Dict[str, Optional[RunResult]]]

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

    return json.loads(stat_res.group(1)), json.loads(stat_res.group(2)), stat_res.group(3), json.loads(stat_res.group(4))


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
    file_name = file_name_match.group(1).split("/")[-1]

    # If we only have 2 full lines, we're not done solving
    if len(stdout) <= 2 or len(stdout[2]) == 0:
        return version, file_name, None

    result_line_i = 2
    if "=====UNKNOWN=====" in stdout[result_line_i]:
        result = Result.UNKNOWN
        result_values = None
    elif "=====UNSATISFIABLE=====" in stdout[result_line_i]:
        result = Result.UNSAT
        result_values = None
    else:
        result = Result.SUCCESS

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

    linear_leq_id_values = defaultdict(lambda: LinearLeq(0, 0))
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
                case "number_of_executions":
                    linear_leq_id_values[linear_leq_id].num_executions = value
                case "number_of_propagations":
                    linear_leq_id_values[linear_leq_id].num_propagations = value

    return version, file_name, RunData(result,
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
            version, file_name, run_data = parse_stdout(exp_dir / "stdout")
            results.append(RunResult(exit_code, wall_time, version, file_name, run_err, run_data))

    return results


def problem_names(*runs: List[RunResult]):
    problems = set()

    for run in runs:
        for res in run:
            problems.add(res.fzn_file_name)

    return list(problems)



def update_results(results: Results, prog_results: List[RunResult], prog_name: str):
    for prog_res in prog_results:
        results[prog_res.fzn_file_name][prog_name] = prog_res


def parse_bench_results():
    intsat_results = parse_results_dir(BASE_DIR / "4" / "0")
    resolution_results = parse_results_dir(BASE_DIR / "4" / "1")

    assert all(map(lambda r: r.run_data is None or (not r.run_data.use_intsat and not r.run_data.skip_nogood_learning), resolution_results))
    assert all(map(lambda r: r.run_data is None or (r.run_data.use_intsat and not r.run_data.skip_nogood_learning), intsat_results))

    results = {
        prob: {}
        for prob in problem_names(resolution_results, intsat_results)
    }

    update_results(results, resolution_results, "resolution")
    update_results(results, intsat_results, "intsat")

    with open('results_out_bench.pkl', 'wb') as results_out:
        pickle.dump(results, results_out, pickle.HIGHEST_PROTOCOL)


def parse_examples_results():
    resolution_results = parse_results_dir(BASE_DIR / "7" / "1")
    intsat_results = parse_results_dir(BASE_DIR / "7" / "0")
    intsat_skip_results = parse_results_dir(BASE_DIR / "8" / "0")

    assert all(map(lambda r: r.run_data is None or (not r.run_data.use_intsat and not r.run_data.skip_nogood_learning), resolution_results))
    assert all(map(lambda r: r.run_data is None or (r.run_data.use_intsat and not r.run_data.skip_nogood_learning), intsat_results))
    assert all(map(lambda r: r.run_data is None or (r.run_data.use_intsat and r.run_data.skip_nogood_learning), intsat_skip_results))

    results = {
        prob: {}
        for prob in problem_names(resolution_results, intsat_skip_results, intsat_results)
    }

    update_results(results, resolution_results, "resolution")
    update_results(results, intsat_results, "intsat")
    update_results(results, intsat_skip_results, "intsat_skip")

    with open('results_out_examples.pkl', 'wb') as results_out:
        pickle.dump(results, results_out, pickle.HIGHEST_PROTOCOL)


def did_fail(res: Optional[RunResult]):
    return ((res is None) or
            (res.exit_code != 0) or
            (res.wall_time > 3600) or
            (res.run_data is None) or
            (res.run_data.result == Result.UNKNOWN))


def examples_results_to_table(results: Results):
    table = []

    for prob in results.keys():
        prob_results = results[prob]
        resolution, intsat, intsat_skip = prob_results.get('resolution'), prob_results.get('intsat'), prob_results.get('intsat_skip')

        if not did_fail(resolution):
            res_conflicts = resolution.run_data.num_conflicts
        else:
            res_conflicts = None

        if not did_fail(intsat_skip):
            intsat_skip_conflicts = intsat_skip.run_data.num_conflicts
            intsat_skip_constraints = intsat_skip.run_data.intsat_learned_constraints
            intsat_skip_fallbacks = intsat_skip.run_data.intsat_fallback_used
            intsat_skip_learned_propagations = sum(map(lambda leq: leq.num_propagations, intsat_skip.run_data.linear_leqs.values()))
        else:
            intsat_skip_conflicts = intsat_skip_constraints = intsat_skip_fallbacks = intsat_skip_learned_propagations = None

        if not did_fail(intsat):
            intsat_conflicts = intsat.run_data.num_conflicts
            intsat_constraints = intsat.run_data.intsat_learned_constraints
            intsat_fallbacks = intsat.run_data.intsat_fallback_used
            intsat_learned_propagations = sum(map(lambda leq: leq.num_propagations, intsat.run_data.linear_leqs.values()))
        else:
            intsat_conflicts = intsat_constraints = intsat_fallbacks = intsat_learned_propagations = None

        if did_fail(resolution) and did_fail(intsat_skip) and did_fail(intsat):
            continue

        conf_values = [res_conflicts, intsat_skip_conflicts, intsat_conflicts]
        conf_values = list(filter(lambda x: x is not None, conf_values))

        best_conf = min(conf_values)
        all_conf_same = len(conf_values) > 1 and all(x == conf_values[0] for x in conf_values)

        table.append([prob, (f"{res_conflicts if res_conflicts is not None else '-'} ({resolution.short_result() if resolution is not None else '-'})", res_conflicts == best_conf and not all_conf_same),
                      (f"{intsat_conflicts if intsat_conflicts is not None else '-'} ({intsat.short_result() if intsat is not None else '-'})", intsat_conflicts == best_conf and not all_conf_same), intsat_constraints, intsat_fallbacks, intsat_learned_propagations,
                      (f"{intsat_skip_conflicts if intsat_skip_conflicts is not None else '-'} ({intsat_skip.short_result() if intsat_skip_conflicts is not None else '-'})", intsat_skip_conflicts == best_conf and not all_conf_same), intsat_skip_constraints, intsat_skip_fallbacks, intsat_skip_learned_propagations])

    return sorted(table, key=lambda r: r[0])


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
        "IntSat": ["\# conf", "\# lear constr", "\# fallb", "\# learn prop"],
        "IntSat skip nogood learning": ["\# conf", "\# lear constr", "\# fallb", "\# learn prop"],
    }
    rendered = template.render({
        "headers": headers,
        "rows": table
    })

    return rendered


def print_errored_problems(results: Results):
    for (prob, progs_res) in results.items():
        for prog_name, prog_res in progs_res.items():
            if prog_res is None:
                continue

            if prog_res.run_error is None:
                continue

            # Ignore errors about unbounded ints/floats
            if (("UnsupportedVariable(\"unbounded int\")" in prog_res.run_error.stderr) or
                    ("UnsupportedVariable(\"float\")" in prog_res.run_error.stderr) or
                    ("floats are not supported" in prog_res.run_error.stderr)):
                continue

            print(f"Found error in {prob} (program {prog_name})")
            print(prog_res.run_error.stderr)
            print("=======")


def verify_solution(result: RunResult):
    if (result.run_error is not None) or (result.run_data is None):
        print(f"Not verifying solution {result.fzn_file_name} with error")
        return

    if result.run_data.result is not Result.SUCCESS:
        print(f"Not verifying solution {result.fzn_file_name} with result {result.run_data.result}")
        return

    if result.run_data.result_values is None:
        print(f"Not verifying solution {result.fzn_file_name} with empty result {result.run_data.result_values}")
        return

    bench_dir_names = list(map(lambda f: f.name, filter(lambda f: f.is_dir(), BENCH_DIR.iterdir())))

    result_dir_name = next((dir_name for dir_name in bench_dir_names if result.fzn_file_name.startswith(dir_name)))
    fzn_path = BENCH_DIR / result_dir_name / result.fzn_file_name

    cmd = ["minizinc", str(fzn_path), "-D", f"\"{result.run_data.result_values}\""]
    res = str(subprocess.check_output(" ".join(cmd), shell=True))

    if (("==UNSATISFIABLE==" in res) or
        ("==UNBOUNDED==" in res) or
        ("==UNSATorUNBOUNDED==" in res) or
        ("==UNKNOWN==" in res) or
        ("==ERROR==" in res)):
        print(f"Not sure if output is correct: {res} \n with for {result.fzn_file_name}")


def verify_solutions(results: Results):
    for progs_res in results.values():
        for prog_res in progs_res.values():
            verify_solution(prog_res)


if __name__ == "__main__":
    parse_examples_results()
    with open('results_out_examples.pkl', 'rb') as results_file:
        results = pickle.load(results_file)
    table = examples_results_to_table(results)
    print(table_to_latex(table))

    # parse_bench_results()
    # with open('results_out_bench.pkl', 'rb') as results_file:
    #     results = pickle.load(results_file)

    # print_errored_problems(results)
    verify_solutions(results)
