from math import ceil

import numpy as np
from jinja2 import Environment, FileSystemLoader, select_autoescape

from parse_output_files import Program, Results

SKIP_PROBLEM_CONFLICTS = 10

BASE_TABLE_ORDER = [Program.RESOLUTION, Program.INTSAT_PUMPKIN, Program.INTSAT_SKIP_PUMPKIN, Program.INTSAT_OG]

PROGRAM_HEADERS = {
    Program.RESOLUTION: ["time", "\# conf"],
    Program.INTSAT_PUMPKIN: ["time", "\# conf", "\# LC", "\# FB", "\# prop"],
    Program.INTSAT_SKIP_PUMPKIN: ["time", "\# conf", "\# LC", "\# FB", "\# prop"],
    Program.INTSAT_OG: ["time", "\# conf", "\# LC"],
}

def results_to_table(results: Results):
    results_programs = { prog_name for prob_results in results.values() for prog_name in prob_results.keys() }
    results_program_order = list(filter(lambda p: p in results_programs, BASE_TABLE_ORDER))

    headers = {
        "": ["Problem"],
        **{ p.value: PROGRAM_HEADERS[p] for p in results_program_order }
    }

    data = []

    for prob_name in sorted(results.keys()):
        prob_results = results[prob_name]

        # Skip problems that all failed
        if all(prob_results.get(p) is None or prob_results[p].failed() for p in results_programs):
            continue

        prob_data = [prob_name]

        # Pre-compute some nr of conflict values
        nr_conf_values = list(map(lambda res: res.run_data.stats.num_conflicts,
                               filter(lambda res: res is not None and res.run_data is not None,
                                      map(lambda p: prob_results.get(p), results_programs))))

        if all(c < SKIP_PROBLEM_CONFLICTS for c in nr_conf_values):
            continue

        best_conf = min(nr_conf_values)
        all_conf_same = len(nr_conf_values) > 1 and all(x == nr_conf_values[0] for x in nr_conf_values)

        for program in results_program_order:
            program_result = prob_results.get(program)

            if program_result is None:
                prob_data.extend([None] * len(PROGRAM_HEADERS[program]))
                continue

            if program_result.failed():
                if program_result.timed_out():
                    # If timed out, show no time
                    prob_data.append(None)
                else:
                    # If another error occurred, show the time in red
                    prob_data.append((round(program_result.wall_time, 2), "color-red"))

                num_conflicts_str = f"- ({program_result.short_result()})"
                prob_data.append((num_conflicts_str, "color-red"))

                extra_fields = len(PROGRAM_HEADERS[program]) - 2
                prob_data.extend([None] * extra_fields)
                continue

            prob_data.append(round(program_result.wall_time, 2))

            num_conflicts_str = f"{program_result.run_data.stats.num_conflicts} ({program_result.short_result()})"
            num_conflicts_best = program_result.run_data.stats.num_conflicts == best_conf and not all_conf_same
            prob_data.append((num_conflicts_str, "text-bold" if num_conflicts_best else ""))

            if program.is_resolution():
                continue

            if program.is_intsat_og():
                prob_data.append(program_result.run_data.stats.intsat_learned_constraints)
                continue

            if program.is_pumpkin_intsat():
                prob_data.append(program_result.run_data.stats.intsat_learned_constraints)
                prob_data.append(program_result.run_data.stats.intsat_fallback_used)
                prob_data.append(sum(map(lambda leq: leq.num_propagations, program_result.run_data.stats.linear_leqs.values())))
                continue

        data.append(prob_data)

    return headers, data


def table_to_latex(headers, data, split_size=None):
    if split_size is not None:
        chunks = ceil(len(data) / split_size)

        rendered = ""
        for data_chunk in np.array_split(np.array(data, dtype=object), chunks):
            rendered += table_to_latex(headers, data_chunk)
            rendered += "\n\n"

        return rendered

    env = Environment(
            loader=FileSystemLoader("./"),
            autoescape=select_autoescape()
        )
    env.tests['tuple'] = lambda v: type(v) is tuple
    template = env.get_template("table_out.j2")

    rendered = template.render({
        "headers": headers,
        "rows": data
    })

    return rendered
