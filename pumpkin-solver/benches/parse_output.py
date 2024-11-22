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


@dataclass
class Stats:
    num_decisions: int
    num_conflicts: int
    num_restarts: int
    num_propagations: int

    intsat_learned_constraints: int
    intsat_learned_constraints_avg_length: float
    intsat_learned_constraints_avg_coeff: float
    intsat_fallback_used: int

    linear_leqs: Dict[int, LinearLeq]


class Result(Enum):
    UNSAT = "unsat"
    UNKNOWN = "unknown"
    SUCCESS = "success"


@dataclass
class Outputs:
    result: Result
    outputs: Optional[List[List[str]]]


@dataclass
class RunData:
    use_intsat: bool
    skip_nogood_learning: bool

    stats: Stats
    outputs: Outputs


@dataclass
class RunResult:
    exit_code: int
    wall_time: int

    bench_version: int
    fzn_file_name: str
    fzn_file_path: Path

    stderr: Optional[str]
    stdout: Optional[str]

    run_data: Optional[RunData]

    def short_result(self):
        if self.stderr is not None:
            return "E"

        if self.run_data is None:
            return "?"

        if self.wall_time > 3600 or self.run_data.outputs.result == Result.UNKNOWN:
            return "T"

        if self.run_data.outputs.result == Result.SUCCESS:
            return "S"

        if self.run_data.outputs.result == Result.UNSAT:
            return "U"


Results = Dict[str, Dict[str, Optional[RunResult]]]

def parse_metrics(metrics_path: Path):
    with open(metrics_path, 'rb') as metrics_file:
        metrics = tomli.load(metrics_file)

    exit_code = metrics['exit_code']
    wall_time = metrics['wall_micros']['secs'] + metrics['wall_micros']['nanos'] / 1e9

    return exit_code, wall_time


def parse_run_info(info_path: Path):
    with open(info_path) as info_file:
        info_lines = info_file.read().split("\n")

    version = int(info_lines[0].split(": ")[1])

    file_path = Path(info_lines[1].split(": ")[1])
    file_name = file_path.stem

    return version, file_path, file_name


def parse_stderr(stderr_path: Path):
    with open(stderr_path) as stderr_file:
        stderr = stderr_file.read()

    return stderr if len(stderr) > 0 else None


def parse_stdout(stdout_path: Path):
    with open(stdout_path) as stdout_file:
        stdout = stdout_file.read()

    return stdout if len(stdout) > 0 else None


def parse_stat_file(stat_path: Path):
    with open(stat_path) as stat_file:
        stats_lines = stat_file.read().split("\n")

    def parse_stat_line(stat_line: str):
        stat_res = re.search("^\$stat\$-I(.+)-SL(.+) (.+)=(.+)$", stat_line)
        if stat_res is None:
            raise RuntimeError(f"Cannot parse line {stat_line}")

        return json.loads(stat_res.group(1)), json.loads(stat_res.group(2)), stat_res.group(3), json.loads(stat_res.group(4))

    use_intsat, skip_nogood_learning, _, _ = parse_stat_line(stats_lines[0])

    linear_leq_id_values = defaultdict(lambda: LinearLeq(0, 0))
    for line_i in range(0, len(stats_lines)):
        line_val = stats_lines[line_i]

        if len(line_val) == 0: continue

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

    return use_intsat, skip_nogood_learning, Stats(
        num_decisions,
        num_conflicts,
        num_restarts,
        num_propagations,
        intsat_learned_constraints,
        intsat_learned_constraints_avg_length,
        intsat_learned_constraints_avg_coeff,
        intsat_fallback_used,
        dict(linear_leq_id_values)
   )


def parse_outputs(outputs_path: Path):
    with open(outputs_path) as outputs_file:
        outputs_lines = outputs_file.read().split("\n")

    if "=====UNKNOWN=====" in outputs_lines[0]:
        result = Result.UNKNOWN
        outputs = None
    elif "=====UNSATISFIABLE=====" in outputs_lines[0]:
        result = Result.UNSAT
        outputs = None
    else:
        result = Result.SUCCESS

        outputs = [[]]

        outputs_line_i = 0
        while "==========" not in outputs_lines[outputs_line_i]:
            if "----------" in outputs_lines[outputs_line_i]:
                outputs.append([])

            outputs[-1].append(outputs_lines[outputs_line_i])
            outputs_line_i += 1

    return Outputs(result, outputs)


def parse_intsat(stderr_path: Path):
    with open(stderr_path) as stderr_file:
        output = stderr_file.read().split("\n")

    file_path = Path(output[1].split(":  ")[1])
    file_name = file_path.stem

    if "Internal error" in output[2]:
        return "intsat", file_name, file_path, "??", None, None

    for stats_line_i in range(5, len(output)):
        line = output[stats_line_i]

        if "Decisions:" in line:
            num_decisions = int(line.split(":")[1].strip())

        if "Conflicts:" in line:
            num_conflicts = int(line.split(":")[1].strip())

        if "Restarts:" in line:
            num_restarts = int(line.split(":")[1].strip())

        if "Total learned Constrs" in line:
            intsat_learned_constraints = int(line.split(":")[1].strip())

        if "Avg. size of learned Ctrs" in line:
            try:
                intsat_learned_constraints_avg_length = int(line.split(":")[1].strip())
            except ValueError:
                intsat_learned_constraints_avg_length = 0

    return "intsat", file_name, file_path, None, None, RunData(
        True,
        True,
        Stats(
            num_decisions,
            num_conflicts,
            num_restarts,
            0,
            intsat_learned_constraints,
            intsat_learned_constraints_avg_length,
            0,
            0,
            dict()
        ),
        Outputs(Result.SUCCESS, None)
    )


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

            if (exp_dir / "intsat.pid.txt").exists():
                version, file_name, file_path, stderr, stdout, run_data = parse_intsat(exp_dir / "stderr")
                results.append(RunResult(exit_code, wall_time, version, file_name, file_path, stderr, stdout, run_data))
            else:
                stderr = parse_stderr(exp_dir / "stderr")
                stdout = parse_stdout(exp_dir / "stdout")

                version, file_path, file_name = parse_run_info(exp_dir / "run_info")

                if stderr is None:
                    use_intsat, skip_nogood_learning, stats = parse_stat_file(exp_dir / "run_stats")
                    outputs = parse_outputs(exp_dir / "run_outputs")
                    run_data = RunData(
                        use_intsat,
                        skip_nogood_learning,
                        stats,
                        outputs,
                    )
                else:
                    run_data = None

                results.append(RunResult(exit_code, wall_time, version, file_name, file_path, stderr, stdout, run_data))

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
    resolution_results = parse_results_dir(BASE_DIR / "13" / "2")
    intsat_results = parse_results_dir(BASE_DIR / "12" / "0")
    intsat_skip_results = parse_results_dir(BASE_DIR / "12" / "1")
    intsat_og_results = parse_results_dir(BASE_DIR / "9" / "0")

    assert all(map(lambda r: r.run_data is None or (not r.run_data.use_intsat and not r.run_data.skip_nogood_learning), resolution_results))
    assert all(map(lambda r: r.run_data is None or (r.run_data.use_intsat and not r.run_data.skip_nogood_learning), intsat_results))
    assert all(map(lambda r: r.run_data is None or (r.run_data.use_intsat and r.run_data.skip_nogood_learning), intsat_skip_results))

    results = {
        prob: {}
        for prob in problem_names(resolution_results, intsat_skip_results, intsat_results, intsat_og_results)
    }

    update_results(results, resolution_results, "resolution")
    update_results(results, intsat_results, "intsat")
    update_results(results, intsat_skip_results, "intsat_skip")
    update_results(results, intsat_og_results, "intsat_og")

    with open('results_out_examples.pkl', 'wb') as results_out:
        pickle.dump(results, results_out, pickle.HIGHEST_PROTOCOL)


def did_fail(res: Optional[RunResult]):
    return ((res is None) or
            (res.exit_code != 0) or
            (res.wall_time > 3600) or
            (res.run_data is None) or
            (res.run_data.outputs.result == Result.UNKNOWN))


def examples_results_to_table(results: Results):
    table = []

    for prob in results.keys():
        prob_results = results[prob]
        resolution, intsat, intsat_skip, intsat_og = prob_results.get('resolution'), prob_results.get('intsat'), prob_results.get('intsat_skip'), prob_results.get('intsat_og')

        if not did_fail(resolution):
            res_time = round(resolution.wall_time, 2)
            res_conflicts = resolution.run_data.stats.num_conflicts
        else:
            res_time = res_conflicts = None

        if not did_fail(intsat_skip):
            intsat_skip_time = round(intsat_skip.wall_time, 2)
            intsat_skip_conflicts = intsat_skip.run_data.stats.num_conflicts
            intsat_skip_constraints = intsat_skip.run_data.stats.intsat_learned_constraints
            intsat_skip_fallbacks = intsat_skip.run_data.stats.intsat_fallback_used
            intsat_skip_learned_propagations = sum(map(lambda leq: leq.num_propagations, intsat_skip.run_data.stats.linear_leqs.values()))
        else:
            intsat_skip_time = intsat_skip_conflicts = intsat_skip_constraints = intsat_skip_fallbacks = intsat_skip_learned_propagations = None

        if not did_fail(intsat):
            intsat_time = round(intsat.wall_time, 2)
            intsat_conflicts = intsat.run_data.stats.num_conflicts
            intsat_constraints = intsat.run_data.stats.intsat_learned_constraints
            intsat_fallbacks = intsat.run_data.stats.intsat_fallback_used
            intsat_learned_propagations = sum(map(lambda leq: leq.num_propagations, intsat.run_data.stats.linear_leqs.values()))
        else:
            intsat_time = intsat_conflicts = intsat_constraints = intsat_fallbacks = intsat_learned_propagations = None

        if not did_fail(intsat_og):
            intsat_og_time = round(intsat_og.wall_time, 2)
            intsat_og_conflicts = intsat_og.run_data.stats.num_conflicts
            intsat_og_constraints = intsat_og.run_data.stats.intsat_learned_constraints
        else:
            intsat_og_time = intsat_og_conflicts = intsat_og_constraints = None

        if did_fail(resolution) and did_fail(intsat_skip) and did_fail(intsat) and did_fail(intsat_og):
            continue

        conf_values = [res_conflicts, intsat_skip_conflicts, intsat_conflicts, intsat_og_conflicts]
        conf_values = list(filter(lambda x: x is not None, conf_values))

        best_conf = min(conf_values)
        all_conf_same = len(conf_values) > 1 and all(x == conf_values[0] for x in conf_values)

        table.append([
            prob,
            res_time, (f"{res_conflicts if res_conflicts is not None else '-'} ({resolution.short_result() if resolution is not None else '-'})", res_conflicts == best_conf and not all_conf_same),
            intsat_time, (f"{intsat_conflicts if intsat_conflicts is not None else '-'} ({intsat.short_result() if intsat is not None else '-'})", intsat_conflicts == best_conf and not all_conf_same), intsat_constraints, intsat_fallbacks, intsat_learned_propagations,
            intsat_skip_time, (f"{intsat_skip_conflicts if intsat_skip_conflicts is not None else '-'} ({intsat_skip.short_result() if intsat_skip_conflicts is not None else '-'})", intsat_skip_conflicts == best_conf and not all_conf_same), intsat_skip_constraints, intsat_skip_fallbacks, intsat_skip_learned_propagations,
            intsat_og_time, (f"{intsat_og_conflicts if intsat_og_conflicts is not None else '-'} ({intsat_og.short_result() if intsat_og is not None else '-'})", intsat_og_conflicts == best_conf and not all_conf_same), intsat_og_constraints
        ])

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
        "Resolution": ["time", "\# conf"],
        "IntSat": ["time", "\# conf", "\# lear constr", "\# fallb", "\# learn prop"],
        "IntSat skip nogood learning": ["time", "\# conf", "\# lear constr", "\# fallb", "\# learn prop"],
        "IntSat OG": ["time", "\# conf", "\# lear constr"],
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
    if (result.stderr is not None) or (result.run_data is None):
        print(f"Not verifying solution {result.fzn_file_name} with error")
        return

    if result.run_data.outputs.result is not Result.SUCCESS:
        print(f"Not verifying solution {result.fzn_file_name} with result {result.run_data.outputs.result}")
        return

    if result.run_data.outputs.outputs is None:
        print(f"Not verifying solution {result.fzn_file_name} with empty result {result.run_data.outputs.outputs}")
        return

    fzn_path = BENCH_DIR / result.fzn_file_path.parent.name / result.fzn_file_name

    # TODO FIX
    cmd = ["minizinc", str(fzn_path), "-D", f"\"{result.run_data.outputs.outputs}\""]
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
    # verify_solutions(results)
