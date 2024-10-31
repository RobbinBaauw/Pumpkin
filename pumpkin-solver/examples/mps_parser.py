import numpy as np
from pysmps.smps_loader import load_mps

(
    name,
    objective_name,
    constraint_names,
    variable_names,
    _,
    constraint_types,
    objective_coefficients,
    A,
    rhs_names,
    rhs,
    bnd_names,
    bnd
) = load_mps("./stein9inf.mps")

A_int = A.astype(np.int32)
rhs_int = rhs['RHS'].astype(np.int32)

var_configs = np.empty((len(variable_names), 2), dtype=np.int32)
var_configs[:, 0] = np.iinfo(np.int32).min
var_configs[:, 1] = np.iinfo(np.int32).max

for bnd_name in bnd_names:
    bnd_obj = bnd[bnd_name]

    if 'LO' in bnd_obj:
        var_configs[:, 0] = bnd_obj['LO']

    if 'UP' in bnd_obj:
        var_configs[:, 1] = bnd_obj['UP']

    if 'FX' in bnd_obj:
        var_configs[:, 0] = bnd_obj['FX']
        var_configs[:, 1] = bnd_obj['FX']

    if 'FR' in bnd_obj:
        print("Parser shouldn't return FR")
        exit(1)

    if 'MI' in bnd_obj:
        print("Parser shouldn't return MI")
        exit(1)

    if 'PI' in bnd_obj:
        print("Parser shouldn't return PI")
        exit(1)

var_configs[var_configs[:, :] == -np.inf] = np.iinfo(np.int32).min
var_configs[var_configs[:, :] == np.inf] = np.iinfo(np.int32).max

for v_idx, v_name in enumerate(variable_names):
    print(f"let {v_name} = solver.new_named_bounded_integer({var_configs[v_idx, 0]}, {var_configs[v_idx, 1]}, \"{v_name}\");")

for c_idx, (c_name, c_type) in enumerate(zip(constraint_names, constraint_types)):
    v_coeffs = A_int[c_idx, :]
    non_zero_idx = v_coeffs.nonzero()[0]

    if c_type == 'G':
        multiplier = -1
    elif c_type == 'L':
        multiplier = 1
    else:
        multiplier = 1

    variables_scaled = ', '.join(map(lambda i: f"{variable_names[i]}.scaled({multiplier * v_coeffs[i]})", non_zero_idx))
    rhs = multiplier * rhs_int[c_idx]

    if c_type == 'E':
        print(f"let _ = solver.add_constraint(constraints::equals(vec![{variables_scaled}], {rhs})).post();")
    else:
        print(f"let _ = solver.add_constraint(constraints::less_than_or_equals(vec![{variables_scaled}], {rhs})).post();")

print(f"vec![{', '.join(variable_names)}]")