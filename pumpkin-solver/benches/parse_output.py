from pathlib import Path
import pickle

from learned_constraint_analysis import run_learned_constraint_analysis
from parse_output_files import parse_results_dir, Program, combine_run_results, Results
from print_output import results_to_table, table_to_latex

BENCH_DIR = Path(__file__).parent / "examples-set"
BASE_DIR = Path(__file__).parent / "results" / "experiments"
BASE_DIR_RELEVANT = Path(__file__).parent / "results" / "experiments_relevant"


def parse_bench_results():
    intsat_results = parse_results_dir(BASE_DIR / "16" / "0", None)
    intsat_results_p2 = parse_results_dir(BASE_DIR / "17" / "0", None)
    resolution_results = parse_results_dir(BASE_DIR / "18" / "0", None)

    assert all(map(lambda r: r.program == Program.RESOLUTION, resolution_results))
    assert all(map(lambda r: r.program == Program.INTSAT_PUMPKIN, intsat_results))

    results = combine_run_results(intsat_results, intsat_results_p2, resolution_results)
    with open('results_out_bench.pkl', 'wb') as results_out:
        pickle.dump(results, results_out, pickle.HIGHEST_PROTOCOL)


def parse_examples_results():
    resolution_results = parse_results_dir(BASE_DIR / "15" / "2", None)
    intsat_results = parse_results_dir(BASE_DIR / "23" / "0", None)
    intsat_skip_results = parse_results_dir(BASE_DIR / "23" / "1", None)
    intsat_og_results = parse_results_dir(BASE_DIR / "9" / "0", None)

    assert all(map(lambda r: r.program == Program.RESOLUTION, resolution_results))
    assert all(map(lambda r: r.program == Program.INTSAT_PUMPKIN, intsat_results))
    assert all(map(lambda r: r.program == Program.INTSAT_SKIP_PUMPKIN, intsat_skip_results))
    assert all(map(lambda r: r.program == Program.INTSAT_OG, intsat_og_results))

    results = combine_run_results(resolution_results, intsat_results, intsat_skip_results, intsat_og_results)
    with open('results_out_examples.pkl', 'wb') as results_out:
        pickle.dump(results, results_out, pickle.HIGHEST_PROTOCOL)


def parse_learned_constraints():
    intsat_constraint_results = parse_results_dir(BASE_DIR / "4" / "0", Program.INTSAT_CONSTRAINT)
    intsat_nogood_results = parse_results_dir(BASE_DIR / "3" / "0", Program.INTSAT_NOGOOD)

    assert all(map(lambda r: r.program == Program.INTSAT_CONSTRAINT, intsat_constraint_results))
    assert all(map(lambda r: r.program == Program.INTSAT_NOGOOD, intsat_nogood_results))

    results = combine_run_results(intsat_constraint_results, intsat_nogood_results)
    with open('results_out_learned_constraints.pkl', 'wb') as results_out:
        pickle.dump(results, results_out, pickle.HIGHEST_PROTOCOL)


if __name__ == "__main__":
    # parse_examples_results()
    # with open('results_out_examples.pkl', 'rb') as results_file:
    #     results: Results = pickle.load(results_file)
    # headers, data = results_to_table(results)
    # print(table_to_latex(headers, data))

    # parse_bench_results()
    # with open('results_out_bench.pkl', 'rb') as results_file:
    #     results: Results = pickle.load(results_file)
    # headers, data = results_to_table(results)
    # print(table_to_latex(headers, data, split_size=90))

    # parse_learned_constraints()
    with open('results_out_learned_constraints.pkl', 'rb') as results_file:
        results: Results = pickle.load(results_file)
    run_learned_constraint_analysis(results)
    # print(results)