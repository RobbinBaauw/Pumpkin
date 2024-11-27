import json
import re
from collections import defaultdict
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Optional, Dict, List
from pip._vendor import tomli


@dataclass
class LinearLeq:
    num_executions: int
    num_propagations: int


@dataclass
class Stats:
    objective: Optional[int]

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


class Program(Enum):
    INTSAT_OG = "IntSat OG"
    INTSAT_PUMPKIN = "IntSat"
    INTSAT_SKIP_PUMPKIN = "IntSat skip nogood learning"
    RESOLUTION = "Resolution"

    def is_pumpkin_intsat(self):
        return self == Program.INTSAT_PUMPKIN or self == Program.INTSAT_SKIP_PUMPKIN

    def is_intsat_og(self):
        return self == Program.INTSAT_OG

    def is_resolution(self):
        return self == Program.RESOLUTION


@dataclass
class Outputs:
    result: Result
    outputs: Optional[List[List[str]]]


@dataclass
class RunData:
    stats: Stats
    outputs: Outputs


@dataclass
class RunResult:
    exit_code: int
    wall_time: int

    bench_version: int
    fzn_file_name: str
    fzn_file_path: Path

    program: Program

    stderr: Optional[str]
    stdout: Optional[str]

    run_data: Optional[RunData]

    def short_result(self):
        if self.stderr is not None:
            return "E"

        if self.wall_time > 3600 :
            return "T"

        if self.run_data is None or self.run_data.outputs.result == Result.UNKNOWN:
            return "?"

        if self.run_data.outputs.result == Result.SUCCESS:
            return "S"

        if self.run_data.outputs.result == Result.UNSAT:
            return "U"

    def timed_out(self):
        return self.wall_time > 3600

    def failed(self):
        return ((self.exit_code != 0) or
                (self.timed_out()) or
                (self.run_data is None) or
                (self.run_data.outputs.result == Result.UNKNOWN))


Results = Dict[str, Dict[Program, Optional[RunResult]]]


def combine_run_results(*runs: List[RunResult]) -> Results:
    results: Results = defaultdict(dict)

    for prog_run_results in runs:
        for prog_run_result in prog_run_results:
            results[prog_run_result.fzn_file_name][prog_run_result.program] = prog_run_result

    return results


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

    _all_solutions = json.loads(info_lines[2].split(": ")[1])
    # _time_limit = json.loads(info_lines[3].split(": ")[1])

    use_intsat = json.loads(info_lines[4].split(": ")[1])
    skip_nogood_learning = json.loads(info_lines[5].split(": ")[1])

    if use_intsat:
        if skip_nogood_learning:
            program = Program.INTSAT_SKIP_PUMPKIN
        else:
            program = Program.INTSAT_PUMPKIN
    else:
        program = Program.RESOLUTION

    return version, file_path, file_name, program


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

    objective = None
    linear_leq_id_values = defaultdict(lambda: LinearLeq(0, 0))
    for line_i in range(0, len(stats_lines)):
        line_val = stats_lines[line_i]

        if len(line_val) == 0: continue

        _, _, stat, value = parse_stat_line(line_val)

        match stat:
            case "objective":
                objective = value
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

    return Stats(
        objective,
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

    return -1, file_name, file_path, None, None, RunData(
        Stats(
            0,
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
                results.append(RunResult(exit_code, wall_time, version,
                                         file_name, file_path,
                                         Program.INTSAT_OG,
                                         stderr, stdout, run_data))
            else:
                stderr = parse_stderr(exp_dir / "stderr")
                stdout = parse_stdout(exp_dir / "stdout")

                version, file_path, file_name, program = parse_run_info(exp_dir / "run_info")

                if stderr is None:
                    stats = parse_stat_file(exp_dir / "run_stats")
                    outputs = parse_outputs(exp_dir / "run_outputs")
                    run_data = RunData(stats, outputs)
                else:
                    run_data = None

                results.append(RunResult(exit_code, wall_time, version,
                                         file_name, file_path,
                                         program,
                                         stderr, stdout, run_data))

    return results
