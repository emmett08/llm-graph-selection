#!/usr/bin/env python3
"""Check LP reductions and test the simulator's decision assumptions."""
from __future__ import annotations

import json
from dataclasses import asdict
from pathlib import Path

import numpy as np
from scipy.optimize import linprog

from paired_calibration import paired_risk_upper
from simulate_workflows import (
    ALPHA, DEADLINE, DELTA, EPSILON, GAMMA, GROUPS, HERE, MU, SCENARIOS,
    SerialRepair, calibration, candidates, metrics, moment_table,
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


def route(coefficients, *, per_group=False, retained=None, proportions=MU):
    groups, choices, scenarios, _ = coefficients.shape
    kept = retained if retained is not None else [list(range(choices)) for _ in range(groups)]
    pairs = [(g, j) for g, indices in enumerate(kept) for j in indices]
    gs, js = np.array(pairs).T
    equality = np.zeros((groups, len(pairs) + 1))
    equality[gs, np.arange(len(pairs))] = 1

    rows, rhs = [], []
    for k in range(scenarios):
        mu = proportions[gs, k]
        rows.append(np.r_[mu * coefficients[gs, js, k, 0], -1.0])
        rhs.append(0.0)
        for h, limit in enumerate((0.0, -GAMMA, DELTA), 1):
            row = coefficients[gs, js, k, h]
            if per_group:
                for g in range(groups):
                    if proportions[g, k] > 0:
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


def group_metrics(weights, moments, scenarios=SCENARIOS):
    moment = np.einsum("gj,gjkv->gkv", weights, moments)
    return [
        dict(group=GROUPS[g], scenario=scenarios[k], completion=a,
             risk=b / a if a > 0 else None, lateness=late)
        for g, rows in enumerate(moment) for k, (a, b, _, late) in enumerate(rows)
    ]


def describe(result, true_moments, *, proportions=MU, scenarios=SCENARIOS):
    if result is None:
        return {"feasible": False}
    objective, weights = result
    scenario = metrics(weights, true_moments, proportions=proportions, scenarios=scenarios)
    return dict(feasible=True, objective=objective, positive_weights=int((weights > CHECK_TOL).sum()),
                true_worst_cost=max(row["cost"] for row in scenario),
                scenarios=scenario, groups=group_metrics(weights, true_moments, scenarios))


def service_limit_violations(outcome):
    violations = []
    if outcome["acceptance"] < GAMMA - CHECK_TOL:
        violations.append("completion")
    if outcome["joint_defect"] - EPSILON * outcome["acceptance"] > CHECK_TOL:
        violations.append("defect")
    if outcome["late"] > DELTA + CHECK_TOL:
        violations.append("lateness")
    return violations


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


def task_mix_sensitivity(moments, baseline):
    """Vary the adverse task shares with the conditional outcomes fixed."""
    nominal, adverse = SCENARIOS
    migration_shares = (0.59, MU[1, 1], 0.61)
    cases = []
    for share in migration_shares:
        proportions = MU.copy()
        proportions[:, 1] = (1 - share, share)
        outcome = metrics(baseline[1], moments, proportions=proportions)[1]
        cases.append(dict(migration_share=share, outcomes=outcome,
                          violated_limits=service_limit_violations(outcome)))
    if [case["violated_limits"] for case in cases] != [["defect"], [], ["completion", "lateness"]]:
        raise AssertionError("Task-mix sensitivity fails to reproduce the binding limits")

    endpoints = (migration_shares[0], migration_shares[-1])
    endpoint_proportions = [(1 - share, share) for share in endpoints]
    proportions = np.array([MU[:, 0], *endpoint_proportions]).T
    scenarios = [nominal] + [f"{adverse}_{p:g}_migration" for p in endpoints]
    endpoint_moments = moments[:, :, [0, 1, 1], :]
    optimum = route(service_coefficients(endpoint_moments), proportions=proportions)
    robust = describe(optimum, endpoint_moments, proportions=proportions, scenarios=scenarios)
    if (not robust["feasible"] or robust["objective"] < baseline[0] - CHECK_TOL
            or any(service_limit_violations(row) for row in robust["scenarios"])):
        raise AssertionError("Robust task-mix solution fails to cover the interval endpoints")
    return dict(baseline_at_adverse_task_mix=cases, robust_migration_interval=endpoints,
                conditional_moments_fixed=True, robust=robust)


def without_immediate_rejection(pool, moments, reject, baseline):
    """Remove the named rejection route and identify any zero-acceptance substitutes."""
    no_reject = [j for j in range(len(pool)) if j != reject]
    result = route(service_coefficients(moments), retained=[no_reject] * len(GROUPS))
    if result is None:
        raise AssertionError("Excluding immediate rejection should leave a feasible restricted pool")
    objective, weights = result
    if objective < baseline[0] - CHECK_TOL or np.any(weights[:, reject] != 0):
        raise AssertionError("Restricted pool result is inconsistent with the baseline")
    zero_acceptance_routes = [
        dict(task_group=GROUPS[g], configuration=pool[j].name, weight=weights[g, j],
             acceptance_by_scenario=dict(zip(SCENARIOS, moments[g, j, :, 0])))
        for g, j in zip(*np.nonzero(weights > CHECK_TOL))
        if np.all(moments[g, j, :, 0] == 0)
    ]
    if not zero_acceptance_routes:
        raise AssertionError("Restricted pool no longer reproduces the zero-acceptance substitute")
    report = describe(result, moments)
    original = metrics(baseline[1], moments)
    outcomes = ("acceptance", "joint_defect", "late")
    if any(not np.isclose(before[key], after[key], rtol=0, atol=CHECK_TOL)
           for before, after in zip(original, report["scenarios"]) for key in outcomes):
        raise AssertionError("Excluding immediate rejection changes the aggregate service outcomes")
    return dict(reoptimised=report, zero_acceptance_routes=zero_acceptance_routes)


def single_call_sensitivity(pool, moments, reject):
    """Vary single-call joint defect probabilities while holding other moments fixed."""
    single = [j for j, candidate in enumerate(pool) if candidate.topology == "single"]
    eligible = [single + [reject]] * len(GROUPS)
    if route(service_coefficients(moments), retained=eligible) is not None:
        raise AssertionError("Original single-call pool should be infeasible at these limits")
    relative_reduction = 0.25
    adjusted = moments.copy()
    adjusted[:, single, :, 1] *= 1 - relative_reduction
    result = route(service_coefficients(adjusted), retained=eligible)
    sensitivity = describe(result, adjusted)
    if (not sensitivity["feasible"]
            or any(service_limit_violations(row) for row in sensitivity["scenarios"])):
        raise AssertionError("Sensitivity should make the single-call pool feasible")
    return dict(relative_joint_defect_reduction=relative_reduction,
                only_joint_defect_coefficient_changed=True,
                eligible_topologies=["single", "reject"],
                original_pool_feasible=False, reoptimised=sensitivity)


def serial_handoff_sensitivity(pool, baseline_moments, baseline):
    """Change how completed serial repairs respond to the review flag."""
    repair = SerialRepair(retention=0.50, false_flag_harm=0.20)
    _, moments, _ = moment_table(pool, serial_repair=repair)
    nonserial = [j for j, candidate in enumerate(pool) if candidate.topology != "serial"]
    if not np.array_equal(moments[:, nonserial], baseline_moments[:, nonserial]):
        raise AssertionError("Changing serial repairs changed another workflow family")
    if not np.allclose(moments[..., 2:], baseline_moments[..., 2:], atol=1e-15, rtol=0):
        raise AssertionError("Changing the repair outcome changed the calls or their timing")

    examples = {}
    draft_defect_probability = 0.0744  # Non-blind, easy nominal task at medium effort.
    for flag, draft_has_defect in [("true_flag", True), ("false_flag", False)]:
        retained, introduced = repair.defect_probabilities(draft_defect_probability, draft_has_defect)
        examples[flag] = dict(retained=retained, introduced=introduced, clean=1 - retained - introduced)

    outcomes = metrics(baseline[1], moments)
    for row in outcomes:
        row["violated_limits"] = service_limit_violations(row)
    if "defect" not in outcomes[SCENARIOS.index("adverse")]["violated_limits"]:
        raise AssertionError("Serial handoff sensitivity should break the baseline's adverse defect limit")
    reoptimised = describe(route(service_coefficients(moments)), moments)
    if not reoptimised["feasible"] or any(service_limit_violations(row) for row in reoptimised["scenarios"]):
        raise AssertionError("The reoptimised rule should meet every service limit")
    return dict(assigned_repair_parameters=asdict(repair),
                nonblind_example=dict(draft_defect_probability=draft_defect_probability,
                                      outcome_after_repair=examples),
                original_rule_under_changed_law=outcomes,
                reoptimised=reoptimised)


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
    timely_baseline["feasible"] = not any(
        service_limit_violations(row) for row in timely_baseline["scenarios"]
    )
    timely_defect_free = {
        row["scenario"]: row["acceptance"] - row["joint_defect"]
        for row in timely_baseline["scenarios"]
    }
    timely_refit = describe(route(service_coefficients(timely)), timely)
    timely_by_group = describe(route(service_coefficients(timely), per_group=True), timely)
    if not timely_by_group["feasible"] or timely_by_group["objective"] < timely_refit["objective"] - CHECK_TOL:
        raise AssertionError("Stricter timely group limits should not reduce the aggregate optimum")

    report = dict(
        evidence="Finite simulation with assigned outcome probabilities and seeded synthetic calibration. No provider calls.",
        settings=dict(defect_limit=EPSILON, completion_floor=GAMMA, lateness_ceiling=DELTA,
                      deadline_seconds=DEADLINE, n_per_uncertain_cell=n, family_alpha=ALPHA),
        dominance=dict(original_per_group=len(pool), retained_per_group=list(map(len, retained)),
                       retained_candidates=[[pool[j].name for j in indices] for indices in retained]),
        baseline=describe(full, moments),
        pruned=describe(reduced, moments),
        group_requirements=describe(route(true_coefficients, per_group=True), moments),
        calibration=analyses,
        queue_sensitivity=queue,
        timely_completion=dict(original_route=timely_baseline, reoptimised=timely_refit,
                               original_defect_free_timely=timely_defect_free,
                               reoptimised_by_group=timely_by_group),
        no_immediate_rejection=without_immediate_rejection(pool, moments, reject, full),
        task_mix_sensitivity=task_mix_sensitivity(moments, full),
        single_call_sensitivity=single_call_sensitivity(pool, moments, reject),
        serial_handoff_sensitivity=serial_handoff_sensitivity(pool, moments, full),
    )
    output = Path(HERE) / "refinement-results.json"
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Wrote {output.name}")


if __name__ == "__main__":
    main()
