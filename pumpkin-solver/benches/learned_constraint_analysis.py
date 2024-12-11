from typing import List, Dict

from parse_output_files import Results, RunResult, LearnedConstraintNogoodTerm, VarId, VarBounds


def should_propagate(domains: Dict[VarId, VarBounds], nogood: List[LearnedConstraintNogoodTerm]):
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

    return true_terms == len(nogood) - 1


def analyze_run(run: RunResult):
    for (prop_id, c) in run.run_data.learned_constraints.items():
        print(f"=> Analyzing propagator {prop_id}")

        # TODO top level analysis

        does_prop, does_not_prop = 0, 0

        for propagation in c.propagates_at_conflict:
            also_propagates = should_propagate(propagation.var_domains, c.nogoods)
            does_prop += also_propagates
            does_not_prop += not also_propagates

        print(f"=>=> Constraint (L{len(c.constraint.lhs)}) {str(c.constraint)}")
        print(f"=>=> Nogood (L{len(c.nogoods)}) {' ∧ '.join(map(lambda n: str(n), c.nogoods))}")
        print(f"=>=> Also propagates {does_prop}/{does_prop+does_not_prop}")


def run_learned_constraint_analysis(results: Results):
    for (prob_name, prob_results) in results.items():
        for (prog_name, prog_results) in prob_results.items():
            if prog_results is None or prog_results.run_data is None:
                continue

            if prog_results.run_data.learned_constraints is None:
                continue

            print(f"Analyzing problem {prob_name} for program {prog_name}")
            analyze_run(prog_results)
