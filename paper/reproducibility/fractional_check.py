#!/usr/bin/env python3
"""Check the single-scenario fixed-budget solution for the appendix candidates."""

from fractions import Fraction as F

import numpy as np
from scipy.optimize import linprog


def main():
    acceptance = (F(1), F(4, 5), F(0), F(9, 10))
    defects = (F(6, 25), F(6, 125), F(0), F(27, 500))
    cost = (F(1), F(3), F(0), F(11, 5))
    late = (F(1, 10), F(1, 20), F(0), F(3, 100))
    budget, completion, deadline = F(13, 10), F(3, 5), F(2, 25)
    expected_weights = (F(3, 26), F(0), F(9, 26), F(7, 13))
    expected_risk = F(123, 1300)
    cost_multiplier, acceptance_multiplier = F(81, 650), F(27, 100)

    # Exact witness and lower bound. For any feasible mix, summing the
    # column inequalities gives b - expected_risk*a >= 0.
    assert sum(expected_weights) == 1
    assert sum(w * a for w, a in zip(expected_weights, acceptance)) == completion
    assert sum(w * b for w, b in zip(expected_weights, defects)) == expected_risk * completion
    assert sum(w * c for w, c in zip(expected_weights, cost)) == budget
    assert sum(w * l for w, l in zip(expected_weights, late)) <= deadline
    assert acceptance_multiplier * completion == cost_multiplier * budget
    assert all(
        b - expected_risk * a >= acceptance_multiplier * a - cost_multiplier * c
        for a, b, c in zip(acceptance, defects, cost)
    )

    a, b, c, l = np.asarray((acceptance, defects, cost, late), dtype=float)
    rows = np.vstack((c, -a, l))
    rhs = np.asarray((budget, -completion, deadline), dtype=float)
    ones = np.ones_like(a)
    options = {"primal_feasibility_tolerance": 1e-10}

    # Charnes–Cooper variables are y = w/a(w) and t = 1/a(w).
    transformed = linprog(
        np.r_[b, 0],
        A_ub=np.column_stack((rows, -rhs)),
        b_ub=np.zeros(len(rhs)),
        A_eq=np.vstack((np.r_[ones, -1], np.r_[a, 0])),
        b_eq=[0, 1],
        bounds=(0, None),
        method="highs",
        options=options,
    )
    assert transformed.success, transformed.message
    assert transformed.x[-1] > 0
    weights = transformed.x[:-1] / transformed.x[-1]
    np.testing.assert_allclose(weights, np.asarray(expected_weights, dtype=float), rtol=0, atol=1e-10)
    assert abs(transformed.fun - float(expected_risk)) < 1e-10

    low, high = 0.0, 1.0
    for _ in range(36):
        middle = (low + high) / 2
        feasible = linprog(
            np.zeros_like(a),
            A_ub=np.vstack((rows, b - middle * a)),
            b_ub=np.r_[rhs, 0],
            A_eq=ones[None, :],
            b_eq=[1],
            bounds=(0, None),
            method="highs",
            options=options,
        )
        if feasible.success:
            high = middle
        elif feasible.status == 2:
            low = middle
        else:
            raise RuntimeError(feasible.message)
    assert abs(transformed.fun - high) < 1e-9

    print(f"Budget {budget}, exact risk {expected_risk}, weights {tuple(map(str, expected_weights))}")
    print(f"Charnes–Cooper: {transformed.fun:.10f}. Bisection: {high:.10f}.")


if __name__ == "__main__":
    main()
