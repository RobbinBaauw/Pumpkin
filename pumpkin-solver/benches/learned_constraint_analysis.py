from itertools import groupby
from typing import List, Dict, Tuple
from matplotlib import pyplot as plt

from parse_output_files import Results, RunResult, LearnedConstraintNogoodTerm, VarId, VarBounds, \
    LearnedConstraintInequality, VarScale, Program


def should_propagate_inequality(domains: Dict[VarId, VarBounds], inequality: LearnedConstraintInequality):
    def get_lower_bound(var_id: VarId, var_scale: VarScale):
        var_lb, var_ub = domains[var_id]

        if var_scale < 0:
            return var_scale * var_ub
        else:
            return var_scale * var_lb

    def get_upper_bound(var_id: VarId, var_scale: VarScale):
        var_lb, var_ub = domains[var_id]

        if var_scale < 0:
            return var_scale * var_lb
        else:
            return var_scale * var_ub

    lb_lhs = sum(map(lambda i: get_lower_bound(i[0], i[1]), inequality.lhs))

    for (var_id, var_scale) in inequality.lhs:
        bound = inequality.rhs - (lb_lhs - get_lower_bound(var_id, var_scale))
        if get_upper_bound(var_id, var_scale) > bound:
            return True

    return False

def should_propagate_nogood(domains: Dict[VarId, VarBounds], nogood: List[LearnedConstraintNogoodTerm]):
    true_terms = 0
    for term in nogood:
        lb, ub = domains[term.var_id]

        match term.op:
            case '>=':
                true_terms += (lb >= term.value)
            case '<=':
                true_terms += (ub <= term.value)
            case '!=':
                true_terms += (lb > term.value)
                true_terms += (ub < term.value)
            case '==':
                true_terms += (lb == term.value == ub)

    return true_terms >= len(nogood) - 1


def analyze_run(run: RunResult):
    percentages = []
    for (prop_id, c) in run.run_data.learned_constraints.items():
        # print(f"=> Analyzing propagator {prop_id}")

        # TODO top level analysis

        does_prop, total_prop = 0, 0

        for propagation in c.propagates_at_conflict:
            if c.is_learned_nogood:
                also_propagates = should_propagate_inequality(propagation.var_domains, c.constraint)
            else:
                also_propagates = should_propagate_nogood(propagation.var_domains, c.nogoods)

            does_prop += also_propagates
            total_prop += 1

        # print(f"=>=> Constraint (L{len(c.constraint.lhs)}) {str(c.constraint)}")
        # print(f"=>=> Nogood (L{len(c.nogoods)}) {' ∧ '.join(map(lambda n: str(n), c.nogoods))}")
        # print(f"=>=> Propagate {does_prop}/{total_prop}")

        if total_prop > 0:
            percentages.append((run.program, run.fzn_file_name, does_prop / total_prop))

    return percentages


def run_learned_constraint_analysis(results: Results):
    percentages = []
    for (prob_name, prob_results) in results.items():
        for (prog_name, prog_results) in prob_results.items():
            if prog_results is None or prog_results.run_data is None:
                continue

            if prog_results.run_data.learned_constraints is None:
                continue

            print(f"Analyzing problem {prob_name} for program {prog_name}")
            percentages.extend(analyze_run(prog_results))

    plot_percentages(percentages)


def plot_percentages(percentages: List[Tuple[Program, str, float]]):
    plt.figure()

    kwargs = dict(alpha=0.5, bins=100, density=True, stacked=True)
    colors = ['r', 'b']

    percentages = sorted(percentages, key=lambda g: g[0].value)
    for i, (k, g) in enumerate(groupby(percentages, lambda g: g[0].value)):
        points = list(map(lambda i: i[2] * 100, g))
        plt.hist(points, label=k, color=colors[i], **kwargs)

    plt.legend(loc="upper left")
    plt.show()
