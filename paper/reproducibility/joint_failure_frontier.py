#!/usr/bin/env python3
"""Reproduce the finite simulator's cost frontier for joint terminal failure."""

from __future__ import annotations

import argparse
import csv
import io
import json
from dataclasses import dataclass
from itertools import pairwise

import numpy as np
from scipy.optimize import linprog

from refine_selection import service_coefficients, service_limit_violations, timely_acceptance
from simulate_workflows import (
    ALPHA, DELTA, EPSILON, GAMMA, HERE, MU, SEED, TOPOLOGIES,
    calibration, candidates, metrics, moment_table, solve,
)


TOLERANCE = 1e-8
PLOT_PERCENTAGES = tuple(range(18, 47))
GRID_CAPS = (0.0, *(percentage / 100 for percentage in PLOT_PERCENTAGES), 1.0)
RANDOMISED = "mixed_routing"
RANDOMISED_WITHOUT_REJECTION = "mixed_routing_without_immediate_rejection"
FAMILY = "one_topology_family_without_immediate_rejection"
FROZEN = "one_frozen_workflow"
CSV_PATH = HERE / "joint_failure_frontier.csv"
FIGURE_PATH = HERE.parent / "figures" / "joint-failure-frontier.tikz.tex"


@dataclass(frozen=True)
class Result:
    cost: float
    failure: float
    choice: str = ""


def failure_flags(rows):
    return (rows[..., 1] == 0) | (rows[..., 2] == 1) | (rows[..., 5] == 1)


def joint_failure(distributions, moments):
    """Compute failure on original terminal rows, with a second check via timely moments."""
    failure = np.empty(moments.shape[:-1])
    for cell, rows in distributions.items():
        failure[cell] = rows[failure_flags(rows), 0].sum()
    timely = timely_acceptance(distributions, moments)
    np.testing.assert_allclose(failure, 1 - timely[..., 0] + timely[..., 1],
                               rtol=0, atol=TOLERANCE)
    return failure


class RoutingProblem:
    """One LP over a fixed column set, with the same group rule in all scenarios."""

    def __init__(self, moments, failure, columns, choice=""):
        self.columns = columns
        self.choice = choice
        self.moments = moments
        self.failure = failure
        groups, _, scenarios, _ = moments.shape
        width = len(columns)
        weighted = service_coefficients(moments)[:, columns, :, :] * MU[:, None, :, None]
        failure_columns = failure[:, columns, :] * MU[:, None, :]
        self.rows = []
        self.rhs = []
        self.failure_rows = []
        for k in range(scenarios):
            for h, limit in enumerate((0.0, 0.0, -GAMMA, DELTA)):
                self.rows.append(np.r_[weighted[:, :, k, h].ravel(), -1.0 if h == 0 else 0.0])
                self.rhs.append(limit)
            self.failure_rows.append(len(self.rows))
            self.rows.append(np.r_[failure_columns[:, :, k].ravel(), 0.0])
            self.rhs.append(1.0)
        membership = np.repeat(np.eye(groups), width, axis=1)
        self.equalities = np.column_stack((membership, np.zeros(groups)))
        self.objective = np.r_[np.zeros(groups * width), 1.0]
        self.bounds = [(0, None)] * (groups * width) + [(None, None)]
        failure_epigraph = np.zeros(len(self.rows))
        failure_epigraph[self.failure_rows] = -1.0
        self.secondary_rows = np.column_stack((self.rows, failure_epigraph))
        self.secondary_rhs = np.array(self.rhs)
        self.secondary_rhs[self.failure_rows] = 0.0
        self.secondary_equalities = np.column_stack((self.equalities, np.zeros(groups)))

    def minimum_cost(self, cap):
        rhs = np.array(self.rhs)
        rhs[self.failure_rows] = cap
        solution = linprog(self.objective, A_ub=self.rows, b_ub=rhs,
                           A_eq=self.equalities, b_eq=np.ones(len(self.equalities)),
                           bounds=self.bounds, method="highs-ds")
        if solution.status == 2:
            return None
        if not solution.success:
            raise RuntimeError(solution.message)

        # Among rules at the minimum cost, minimise the worst-scenario failure rate.
        secondary_objective = np.r_[np.zeros(len(self.objective)), 1.0]
        secondary = linprog(
            secondary_objective, A_ub=self.secondary_rows, b_ub=self.secondary_rhs,
            A_eq=self.secondary_equalities, b_eq=np.ones(len(self.equalities)),
            bounds=self.bounds[:-1] + [(None, solution.fun), (0.0, cap)], method="highs-ds",
        )
        if not secondary.success:
            raise RuntimeError(secondary.message)

        # Verify each scenario by averaging the original outcome coefficients.
        weights = np.zeros(self.moments.shape[:2])
        weights[:, self.columns] = secondary.x[:-2].reshape(weights.shape[0], len(self.columns))
        np.testing.assert_allclose(weights.sum(axis=1), 1, rtol=0, atol=TOLERANCE)
        if np.min(weights) < -TOLERANCE:
            raise ArithmeticError("Routing probabilities must be nonnegative")
        observed = metrics(weights, self.moments)
        if any(service_limit_violations(row) for row in observed):
            raise ArithmeticError("A routing rule violates an original service limit")
        failures_by_scenario = np.einsum("gk,gj,gjk->k", MU, weights, self.failure)
        cost = max(row["cost"] for row in observed)
        worst_failure = float(np.max(failures_by_scenario))
        if (worst_failure > cap + TOLERANCE or abs(cost - solution.fun) > TOLERANCE
                or abs(worst_failure - secondary.fun) > TOLERANCE):
            raise ArithmeticError("A routing rule fails the independent scenario check")
        return Result(cost, worst_failure, self.choice)


def check_cost_gap(pool, distributions, moments, cost_caps, failure):
    """Exercise both bounds on the original simulator's fixed calibration draw."""
    n = 20_000
    means, _, _, counts = calibration(distributions, cost_caps, n, SEED + 1)
    observed_failure = np.zeros_like(failure)
    for g, j, k, outcome, count in counts:
        observed_failure[g, j, k] += count * failure_flags(distributions[g, j, k][outcome])
    observed_failure /= n

    rejected = np.asarray([candidate.topology == "reject" for candidate in pool])
    known = np.broadcast_to(rejected[None, :, None], failure.shape)
    endpoints = 10 * np.count_nonzero(~known)
    radius = np.sqrt(np.log(endpoints / ALPHA) / (2 * n))
    ranges = np.ones_like(moments)
    ranges[..., 2] = cost_caps
    low = np.maximum(0, means - ranges * radius)
    high = np.minimum(ranges, means + ranges * radius)
    failure_low = np.maximum(0, observed_failure - radius)
    failure_high = np.minimum(1, observed_failure + radius)
    low[known] = high[known] = moments[known]
    failure_low[known] = failure_high[known] = failure[known]
    if (np.any(low > moments + TOLERANCE) or np.any(moments > high + TOLERANCE)
            or np.any(failure_low > failure + TOLERANCE)
            or np.any(failure > failure_high + TOLERANCE)):
        raise ArithmeticError("The archived draw does not cover the simulated means")

    optimistic = low.copy()
    optimistic[..., 0] = high[..., 0]
    conservative = high.copy()
    conservative[..., 0] = low[..., 0]
    columns = list(range(len(pool)))
    outer = RoutingProblem(optimistic, failure_low, columns)
    population = RoutingProblem(moments, failure, columns)
    inner = RoutingProblem(conservative, failure_high, columns)
    lower = outer.minimum_cost(.30)
    exact = population.minimum_cost(.30)
    upper = inner.minimum_cost(.30)
    if lower is None or exact is None or upper is None:
        raise ArithmeticError("The archived draw should give both cost bounds at 30%")
    if lower.cost > exact.cost + TOLERANCE or exact.cost > upper.cost + TOLERANCE:
        raise ArithmeticError("The cost bounds failed to enclose the 30% optimum")
    if population.minimum_cost(.19) is None or inner.minimum_cost(.19) is not None:
        raise ArithmeticError("Expected population feasibility and no conservative rule at 19%")
    print(f"Two-sided calibration at 30%: lower {lower.cost:.8f}, "
          f"exact {exact.cost:.8f}, upper {upper.cost:.8f}, "
          f"gap bound {upper.cost - lower.cost:.8f} USD per attempt.")


def feasible_frozen_workflows(pool, moments, failure):
    """Evaluate each single workflow directly under each scenario's task mix."""
    workflow_moments = np.einsum("gk,gjkv->jkv", MU, moments)
    workflow_failure = np.einsum("gk,gjk->jk", MU, failure)
    eligible = []
    for j, candidate in enumerate(pool):
        if candidate.topology == "reject":
            continue
        acceptance, defects, cost, late = workflow_moments[j].T
        if (np.any(acceptance < GAMMA - TOLERANCE)
                or np.any(defects > EPSILON * acceptance + TOLERANCE)
                or np.any(late > DELTA + TOLERANCE)):
            continue
        eligible.append(Result(float(max(cost)), float(max(workflow_failure[j])), candidate.name))
    return eligible


def cheapest(options):
    return min((option for option in options if option is not None),
               key=lambda option: (option.cost, option.failure, option.choice), default=None)


def comparisons(problems, family_problems, frozen, caps):
    rows = {}
    for cap in caps:
        rows[cap] = {
            RANDOMISED: problems[RANDOMISED].minimum_cost(cap),
            RANDOMISED_WITHOUT_REJECTION: problems[RANDOMISED_WITHOUT_REJECTION].minimum_cost(cap),
            FAMILY: cheapest(problem.minimum_cost(cap) for problem in family_problems),
            FROZEN: cheapest(option for option in frozen if option.failure <= cap + TOLERANCE),
        }
        for broader, narrower in pairwise(rows[cap].values()):
            if narrower and (broader is None or broader.cost > narrower.cost + TOLERANCE):
                raise ArithmeticError("A restricted policy class improved the optimum")
    for tight, loose in pairwise(rows.values()):
        for method, tight_result in tight.items():
            if tight_result and (loose[method] is None
                                 or tight_result.cost < loose[method].cost - TOLERANCE):
                raise ArithmeticError("Relaxing the failure limit increased the optimum")
    return rows


def render_csv(rows):
    stream = io.StringIO(newline="")
    writer = csv.writer(stream, lineterminator="\n")
    writer.writerow(("failure_cap", "policy_class", "feasible", "minimum_cost_usd_per_attempt",
                     "attained_worst_failure_rate", "selected_family_or_workflow"))
    for cap, choices in rows.items():
        for method, result in choices.items():
            values = (f"{result.cost:.10f}", f"{result.failure:.10f}", result.choice) if result else ("", "", "")
            writer.writerow((f"{cap:.10f}", method, str(result is not None).lower(), *values))
    return stream.getvalue()


def x_coordinate(rate):
    lower, upper = PLOT_PERCENTAGES[0], PLOT_PERCENTAGES[-1]
    return 16 + 100 * (100 * rate - lower) / (upper - lower)


def y_coordinate(cost):
    return 15 + 53 * (1_000 * cost - 35) / (82 - 35)


def point(rate, cost):
    return f"({x_coordinate(rate):.4f},{y_coordinate(cost):.4f})"


FIGURE_START = r"""% Generated by paper/reproducibility/joint_failure_frontier.py.
% Required packages: tikz
% Required TikZ libraries: none
% Target width: 160 mm maximum
% Minimum text size: 9 pt
% Colour/greyscale intent: colour and greyscale
% All marks represent calculations from the finite simulator.
\begingroup
\definecolor{FailureMixed}{HTML}{215178}
\definecolor{FailureNoReject}{HTML}{8C4814}
\definecolor{FailureFamily}{HTML}{3A634D}
\definecolor{FailureFrozen}{HTML}{3C3C3C}
\begin{tikzpicture}[
  x=1mm,y=1mm,
  font=\fontsize{9.5}{11.5}\selectfont,
  axis/.style={line width=.5pt},
  grid/.style={draw=black!19,line width=.4pt},
  guide/.style={draw=black!45,line width=.5pt,densely dashed},
  family mark/.style={draw=FailureFamily,fill=FailureFamily,line width=.4pt},
  no reject mark/.style={draw=FailureNoReject,line width=.6pt},
  frozen step/.style={draw=FailureFrozen,line width=.65pt},
  direct/.style={anchor=west,align=left,inner sep=0pt},
]
\path[use as bounding box] (0,0) rectangle (159,78);
"""


def render_figure(rows, frozen, baseline):
    lines = [FIGURE_START]
    for expense in range(40, 81, 10):
        y = y_coordinate(expense / 1_000)
        lines.extend((
            rf"\draw[grid] (16,{y:.4f}) -- (116,{y:.4f});",
            rf"\node[anchor=east] at (15,{y:.4f}) {{{expense}}};",
        ))
    for percentage in range(20, 46, 5):
        x = x_coordinate(percentage / 100)
        lines.extend((
            rf"\draw[axis] ({x:.4f},15) -- ({x:.4f},13.6);",
            rf"\node[anchor=north] at ({x:.4f},13) {{{percentage}}};",
        ))
    cap30 = x_coordinate(0.30)
    lines.extend((
        rf"\draw[guide] ({cap30:.4f},15) -- ({cap30:.4f},68);",
        rf"\node[anchor=south] at ({cap30:.4f},68.25) {{30\% cap}};",
        r"\draw[axis] (16,68) -- (16,15) -- (116,15);",
        r"\node[anchor=west] at (13,76) {Minimum worst-scenario expected cost (USD per 1,000 attempts)};",
        r"\node at (66,4) {Allowed failure rate per scenario (\%)};",
    ))

    # The continuous horizontal segments are exact for the frozen choices.
    # Every other plotted mark is a solved grid point, with infeasible caps omitted.
    steps = []
    for choice in sorted(frozen, key=lambda value: (value.failure, value.cost, value.choice)):
        if not steps or choice.cost < steps[-1].cost - TOLERANCE:
            steps.append(choice)
    lower_cap, upper_cap = PLOT_PERCENTAGES[0] / 100, PLOT_PERCENTAGES[-1] / 100
    for number, choice in enumerate(steps):
        start = max(choice.failure, lower_cap)
        stop = min(steps[number + 1].failure if number + 1 < len(steps) else 1, upper_cap)
        if start <= stop:
            lines.append(rf"\draw[frozen step] {point(start, choice.cost)} -- {point(stop, choice.cost)};")
            if start == choice.failure:
                lines.append(rf"\fill[FailureFrozen] {point(start, choice.cost)} circle[radius=1.35pt];")

    for percentage in PLOT_PERCENTAGES:
        rate = percentage / 100
        choices = rows[rate]
        for method in (RANDOMISED_WITHOUT_REJECTION, FAMILY, RANDOMISED):
            result = choices[method]
            if result is None:
                continue
            location = point(rate, result.cost)
            if method == RANDOMISED_WITHOUT_REJECTION:
                lines.append(
                    rf"\draw[no reject mark] {location} ++(-.7,-.7) rectangle ++(1.4,1.4);"
                )
            elif method == FAMILY:
                lines.append(
                    rf"\draw[family mark] {location} ++(0,.8) -- ++(-.75,-1.35) -- ++(1.5,0) -- cycle;"
                )
            else:
                lines.append(rf"\fill[FailureMixed] {location} circle[radius=1.12pt];")

    lines.append(rf"\draw[FailureMixed,line width=.65pt] {point(baseline.failure, baseline.cost)} circle[radius=2pt];")
    labels = (
        (FROZEN, r"One frozen workflow", "FailureFrozen"),
        (FAMILY, r"One topology family", "FailureFamily"),
        (RANDOMISED_WITHOUT_REJECTION, r"Mixed without\\immediate rejection", "FailureNoReject"),
        (RANDOMISED, r"Mixed routing", "FailureMixed"),
    )
    for method, label, colour in labels:
        endpoint = rows[upper_cap][method]
        if endpoint:
            y = y_coordinate(endpoint.cost)
            lines.append(rf"\node[direct,text={colour}] at (119,{y:.4f}) {{{label}}};")
    lines.extend((r"\end{tikzpicture}", r"\endgroup", ""))
    return "\n".join(lines)


def write_or_check(path, content, check):
    data = content.encode("utf-8")
    if check and path.read_bytes() != data:
        raise SystemExit(f"{path.name} differs from the reproducible output")
    if not check:
        path.write_bytes(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--failure-cap", type=float, metavar="TAU",
                      help="print the comparison at one cap in [0, 1]")
    mode.add_argument("--check", action="store_true",
                      help="compare the generated CSV and TikZ source with the checkout")
    args = parser.parse_args()
    if args.failure_cap is not None and not 0 <= args.failure_cap <= 1:
        parser.error("--failure-cap must be between 0 and 1")

    registry = json.loads((HERE / "model-registry.json").read_text())
    pool = candidates(registry)
    distributions, moments, cost_caps = moment_table(pool)
    failure = joint_failure(distributions, moments)
    problems = {
        RANDOMISED: RoutingProblem(moments, failure, list(range(len(pool)))),
        RANDOMISED_WITHOUT_REJECTION: RoutingProblem(
            moments, failure, [j for j, candidate in enumerate(pool) if candidate.topology != "reject"]
        ),
    }
    family_problems = [
        RoutingProblem(moments, failure,
                       [j for j, candidate in enumerate(pool) if candidate.topology == topology],
                       choice=topology)
        for topology in TOPOLOGIES
    ]
    frozen = feasible_frozen_workflows(pool, moments, failure)
    caps = GRID_CAPS if args.failure_cap is None else (args.failure_cap,)
    rows = comparisons(problems, family_problems, frozen, caps)

    if args.failure_cap is not None:
        print(render_csv(rows), end="")
        return

    original = solve(moments)
    np.testing.assert_allclose(rows[1.0][RANDOMISED].cost, original.fun, rtol=0, atol=TOLERANCE)
    original_failures = np.einsum("gk,gj,gjk->k", MU, original.weights, failure)
    baseline = Result(original.fun, float(np.max(original_failures)))
    check_cost_gap(pool, distributions, moments, cost_caps, failure)
    write_or_check(CSV_PATH, render_csv(rows), args.check)
    write_or_check(FIGURE_PATH, render_figure(rows, frozen, baseline), args.check)
    print("Joint terminal failure, nested policy classes and frontier outputs verified.")


if __name__ == "__main__":
    main()
