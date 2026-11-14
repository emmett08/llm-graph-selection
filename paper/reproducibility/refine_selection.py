#!/usr/bin/env python3
"""Check exact LP reductions and optional service requirements on the paper's simulator."""
from __future__ import annotations

import json
from pathlib import Path

import numpy as np
from scipy.optimize import linprog

from paired_calibration import paired_risk_upper
from simulate_workflows import (
    ALPHA, DEADLINE, DELTA, EPSILON, GAMMA, GROUPS, HERE, MU, SCENARIOS,
    calibration, candidates, metrics, moment_table,
)

CHECK_TOL = 1e-8


def service_coefficients(moments):
    acceptance, defects, cost, late = np.moveaxis(moments, -1, 0)
    return np.stack((cost, defects - EPSILON * acceptance, -acceptance, late), axis=-1)


def prune_columns(coefficients):
    """Drop group-specific columns dominated across every scenario and constraint."""
    kept = []
    for group in coefficients:
        columns = group.reshape(group.shape[0], -1)
        survivors = []
        for j, column in enumerate(columns):
            no_worse = np.all(columns <= column, axis=1)
            strict = np.any(columns < column, axis=1)
            duplicate = np.all(columns == column, axis=1)
            earlier = np.arange(len(columns)) < j
            if not np.any(no_worse & (strict | (duplicate & earlier))):
                survivors.append(j)
        kept.append(survivors)
    return kept


def route(coefficients, *, per_group=False, retained=None):
    groups, choices, scenarios, _ = coefficients.shape
    kept = retained if retained is not None else [list(range(choices)) for _ in range(groups)]
    pairs = [(g, j) for g, indices in enumerate(kept) for j in indices]
    gs, js = np.array(pairs).T
    equality = np.zeros((groups, len(pairs) + 1))
    equality[gs, np.arange(len(pairs))] = 1

    rows, rhs = [], []
    for k in range(scenarios):
        mu = MU[gs, k]
        rows.append(np.r_[mu * coefficients[gs, js, k, 0], -1.0])
        rhs.append(0.0)
        for h, limit in enumerate((0.0, -GAMMA, DELTA), 1):
            row = coefficients[gs, js, k, h]
            if per_group:
                for g in range(groups):
                    if MU[g, k] > 0:
                        rows.append(np.r_[row * (gs == g), 0.0])
                        rhs.append(limit)
            else:
                rows.append(np.r_[mu * row, 0.0])
                rhs.append(limit)

    objective = np.r_[np.zeros(len(pairs)), 1.0]
    solution = linprog(
        objective, A_ub=rows, b_ub=rhs, A_eq=equality, b_eq=np.ones(groups),
        bounds=[(0, None)] * len(pairs) + [(None, None)], method="highs-ds",
    )
    if solution.status == 2:
        return None
    if not solution.success:
        raise RuntimeError(solution.message)
    if (np.max(np.asarray(rows) @ solution.x - rhs) > CHECK_TOL
            or np.max(np.abs(equality @ solution.x - 1)) > CHECK_TOL
            or np.min(solution.x[:-1]) < -CHECK_TOL):
        raise ArithmeticError("Solver result fails the deployment constraints")
    weights = np.zeros((groups, choices))
    weights[gs, js] = solution.x[:-1]
    return solution.fun, weights


def group_metrics(weights, moments):
    moment = np.einsum("gj,gjkv->gkv", weights, moments)
    return [
        dict(group=GROUPS[g], scenario=SCENARIOS[k], completion=a,
             risk=b / a if a > 0 else None, lateness=late)
        for g, rows in enumerate(moment) for k, (a, b, _, late) in enumerate(rows)
    ]


def describe(result, true_moments):
    if result is None:
        return {"feasible": False}
    objective, weights = result
    scenario = metrics(weights, true_moments)
    return dict(feasible=True, objective=objective, positive_weights=int((weights > CHECK_TOL).sum()),
                true_worst_cost=max(row["cost"] for row in scenario),
                scenarios=scenario, groups=group_metrics(weights, true_moments))


def a_y_counts(distributions, sample_counts, cell_shape):
    a_count = np.zeros(cell_shape, dtype=np.int64)
    y_count = np.zeros_like(a_count)
    for g, j, k, h, count in sample_counts:
        accepted, defective = distributions[g, j, k][h, 1:3]
        a_count[g, j, k] += int(accepted) * count
        y_count[g, j, k] += int(defective) * count
    return a_count, y_count


def timely_acceptance(distributions, moments):
    timely = moments.copy()
    for index, outcomes in distributions.items():
        on_time = outcomes[:, 5] == 0
        timely[index][:2] = (outcomes[:, 0] * on_time) @ outcomes[:, 1:3]
    return timely


def queue_envelope(distributions, moments, reject_index, delay):
    bounded = moments.copy()
    for (g, j, k), outcomes in distributions.items():
        if j != reject_index:
            late = outcomes[:, 4] + delay > DEADLINE
            bounded[g, j, k, 3] = outcomes[:, 0] @ late
    return bounded


def main():
    registry = json.loads((HERE / "model-registry.json").read_text())
    data = json.loads((HERE / "simulation-results.json").read_text())
    pool = candidates(registry)
    distributions, moments, cost_cap = moment_table(pool)
    true_coefficients = service_coefficients(moments)
    retained = prune_columns(true_coefficients)
    full = route(true_coefficients)
    reduced = route(true_coefficients, retained=retained)
    if not np.isclose(full[0], data["known_coefficient_optimum"]["cost"], atol=CHECK_TOL, rtol=0):
        raise AssertionError("Failed to recover the reported baseline")
    if not np.isclose(full[0], reduced[0], atol=1e-10, rtol=0):
        raise AssertionError("Reduced LP changes the optimum")
    if np.count_nonzero(reduced[1] > CHECK_TOL) > len(GROUPS) + 4 * len(SCENARIOS) - 1:
        raise AssertionError("Basic solution exceeds support bound")

    n = data["calibration"]["n_per_cell"]
    means, endpoints, radius, sample_counts = calibration(distributions, cost_cap, n, data["seed"] + 1)
    a_count, y_count = a_y_counts(distributions, sample_counts, cost_cap.shape)
    uncertain = cost_cap > 0
    p = ALPHA / (4 * np.count_nonzero(uncertain))
    if not (np.allclose(a_count / n, means[..., 0]) and np.allclose(y_count / n, means[..., 1])):
        raise AssertionError("Paired counts disagree with the calibration means")

    separate = service_coefficients(endpoints)
    risk_mean = means[..., 1] - EPSILON * means[..., 0]
    paired_hoeffding = separate.copy()
    paired_hoeffding[..., 1] = np.where(
        uncertain, np.minimum(1 - EPSILON, risk_mean + radius), risk_mean,
    )
    paired_bernstein = separate.copy()
    paired_bernstein[..., 1] = np.where(
        uncertain, paired_risk_upper(a_count, y_count, n, EPSILON, p), risk_mean,
    )
    analyses = {
        name: dict(coverage_in_this_sample=bool(np.all(coeff >= true_coefficients - 1e-12)),
                   aggregate=describe(route(coeff), moments),
                   by_group=describe(route(coeff, per_group=True), moments))
        for name, coeff in [
            ("separate_hoeffding", separate),
            ("paired_hoeffding", paired_hoeffding),
            ("paired_empirical_bernstein", paired_bernstein),
        ]
    }
    if not np.isclose(analyses["separate_hoeffding"]["aggregate"]["objective"],
                      data["calibration"]["cost_bound"], atol=CHECK_TOL, rtol=0):
        raise AssertionError("Failed to reproduce separate calibration baseline")

    reject = next(j for j, candidate in enumerate(pool) if candidate.topology == "reject")
    queue = []
    if not np.array_equal(queue_envelope(distributions, moments, reject, 0)[..., :3], moments[..., :3]):
        raise AssertionError("Queue envelope changes non-lateness coefficients")
    for delay in (0, 5, 30, 50):
        bounded = queue_envelope(distributions, moments, reject, delay)
        if delay == 0 and not np.allclose(bounded, moments, atol=1e-13, rtol=0):
            raise AssertionError("Zero-delay envelope fails to recover baseline")
        before = max(row["late"] for row in metrics(full[1], bounded))
        queue.append(dict(delay_seconds=delay, baseline_worst_lateness=before,
                          reoptimised=describe(route(service_coefficients(bounded)), bounded)))

    timely = timely_acceptance(distributions, moments)
    timely_baseline = describe(full, timely)
    timely_baseline["feasible"] = all(
        row["acceptance"] >= GAMMA - CHECK_TOL
        and row["joint_defect"] - EPSILON * row["acceptance"] <= CHECK_TOL
        and row["late"] <= DELTA + CHECK_TOL
        for row in timely_baseline["scenarios"]
    )
    timely_refit = describe(route(service_coefficients(timely)), timely)

    report = dict(
        evidence="Stipulated finite simulator, seeded calibration, no provider executions",
        settings=dict(defect_limit=EPSILON, completion_floor=GAMMA, lateness_ceiling=DELTA,
                      deadline_seconds=DEADLINE, n_per_uncertain_cell=n, family_alpha=ALPHA),
        dominance=dict(original_per_group=len(pool), retained_per_group=list(map(len, retained)),
                       retained_candidates=[[pool[j].name for j in indices] for indices in retained]),
        baseline=describe(full, moments),
        pruned=describe(reduced, moments),
        group_requirements=describe(route(true_coefficients, per_group=True), moments),
        calibration=analyses,
        queue_sensitivity=queue,
        timely_completion=dict(original_route=timely_baseline, reoptimised=timely_refit),
    )
    output = Path(HERE) / "refinement-results.json"
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Wrote {output.name}")


if __name__ == "__main__":
    main()
