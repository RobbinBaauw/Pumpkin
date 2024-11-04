import re

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
) = load_mps("./ponderthis0517-inf.mps")

int32_min = np.iinfo(np.int32).min + 1
int32_max = np.iinfo(np.int32).max - 1

if len(rhs_names) != 1 or len(bnd_names) != 1:
    print("More than 1 name not supported")
    exit(1)


def check_inf(arr, d):
    index = tuple(map(lambda i: slice(None), range(d)))
    any_inf = np.any(arr[arr[index] == -np.inf]) or np.any(arr[arr[index] == np.inf])
    if any_inf:
        print("No inf bounds supported")
        exit(1)

# Replace infs by concrete values
check_inf(A, 2)
check_inf(rhs[rhs_names[0]], 1)
check_inf(rhs[rhs_names[0]], 1)
for v in bnd[bnd_names[0]].values(): check_inf(v, 1)

# Check whether concrete values can be safely cast to int32
if (not np.all(np.mod(A, 1) == 0) or
        not np.all(np.mod(rhs[rhs_names[0]], 1) == 0) or
        not all(map(lambda v: np.all(np.mod(v, 1) == 0), bnd[bnd_names[0]].values()))):
    print("Not fully integer!")
    exit(1)

# Perform the cast
A = A.astype(np.int32)
rhs[rhs_names[0]] = rhs[rhs_names[0]].astype(np.int32)
for (k, v) in bnd[bnd_names[0]].items(): bnd[bnd_names[0]][k] = v.astype(np.int32)

variable_names = list(map(lambda l: re.sub('[^a-zA-Z0-9]', '_', l), variable_names))

# Create the initial bounds for the variables
var_bounds = np.empty((len(variable_names), 2), dtype=np.int32)
var_bounds[:, 0] = int32_min
var_bounds[:, 1] = int32_max

for bnd_name in bnd_names:
    bnd_obj = bnd[bnd_name]

    if 'LO' in bnd_obj:
        var_bounds[:, 0] = bnd_obj['LO']

    if 'UP' in bnd_obj:
        var_bounds[:, 1] = bnd_obj['UP']

    if 'FX' in bnd_obj:
        var_bounds[:, 0] = bnd_obj['FX']
        var_bounds[:, 1] = bnd_obj['FX']

    if 'FR' in bnd_obj:
        print("Parser shouldn't return FR")
        exit(1)

    if 'MI' in bnd_obj:
        print("Parser shouldn't return MI")
        exit(1)

    if 'PI' in bnd_obj:
        print("Parser shouldn't return PI")
        exit(1)

for v_idx, v_name in enumerate(variable_names):
    print(f"let {v_name} = solver.new_named_bounded_integer({var_bounds[v_idx, 0]}, {var_bounds[v_idx, 1]}, \"{v_name}\");")

for c_idx, (c_name, c_type) in enumerate(zip(constraint_names, constraint_types)):
    v_coeffs = A[c_idx, :]
    non_zero_idx = v_coeffs.nonzero()[0]

    if c_type == 'G':
        multiplier = -1
    elif c_type == 'L':
        multiplier = 1
    else:
        multiplier = 1

    variables_scaled = ', '.join(map(lambda i: f"{variable_names[i]}.scaled({multiplier * v_coeffs[i]})", non_zero_idx))
    rhs_mult = multiplier * rhs[rhs_names[0]][c_idx]

    if c_type == 'E':
        print(f"let _ = solver.add_constraint(constraints::equals(vec![{variables_scaled}], {rhs_mult})).post();")
    else:
        print(f"let _ = solver.add_constraint(constraints::less_than_or_equals(vec![{variables_scaled}], {rhs_mult})).post();")

print(f"vec![{', '.join(variable_names)}]")