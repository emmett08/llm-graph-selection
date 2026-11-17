# LLM Workflow Optimiser

The Rust library and command-line interface implement the cost and task-failure comparison in Emmett Miller's [*Choosing LLM workflow topologies under cost and failure constraints*](paper/main.tex). Select a routing rule across topology families, restrict it to one family, or use one configuration for every task.

For each task group, the optimiser chooses the probability of selecting each complete workflow. It uses the same routing probabilities in every operating scenario.
The fixed input determines which outcome the optimiser minimises.

| Decision | Worst-scenario objective |
| --- | --- |
| Cost at a task-failure ceiling | Expected cost per submitted task |
| Task failure within a cost budget | Probability that a submitted task fails |
| Conditional defect risk within a cost budget | Fraction of accepted outputs containing a material defect |

When success requires timely defect-free acceptance, a task fails if any of those conditions is unmet. The optimiser counts each submitted task once. Costs include unsuccessful attempts and any required fallback. For another service obligation, supply the measured failure probability directly.

## Repository layout

| Path | Contents |
| --- | --- |
| [`src/`](src/) | Rust implementation with command-line entry point |
| [`tests/`](tests/) | Rust checks against the paper's results |
| [`fixtures/`](fixtures/) | Synthetic inputs used throughout the Rust examples, including tests |
| [`paper/`](paper/) | Manuscript package with complete build sources |
| [`paper/reproducibility/`](paper/reproducibility/) | Reproduction package containing the simulation evidence |

See [the paper instructions](paper/README.md).

## Run locally

The minimum Rust version is 1.85. Once Rust is installed, the crate builds offline using only the standard library.

```sh
cargo test --offline
cargo run --offline -- --help
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv --failure 0.30 --prune
```

At a 30% failure ceiling, the synthetic example has minimum worst-scenario cost USD 0.04454362 per submitted task. The report lists routing probabilities and service outcomes by scenario and observed task group. Omitting `--failure` gives the original cost comparison at USD 0.03879834.

```sh
# Restrict routing to one topology family and exclude immediate rejection.
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv \
  --failure 0.30 --routing family --families fixtures/population-moments.csv --exclude reject

# Select one fixed configuration for every task group.
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv \
  --failure 0.30 --routing fixed --exclude reject

# Minimise worst-scenario failure subject to a per-scenario budget.
cargo run --offline -- failure fixtures/population-moments.csv fixtures/scenario-weights.csv \
  --budget 0.04454363

# Compare conservative and optimistic costs using five jointly calibrated means.
cargo run --offline -- cost-gap fixtures/calibration-tallies.csv fixtures/scenario-weights.csv \
  --input failure-calibration --failure 0.30
```

The one-family and fixed comparisons give USD 0.05760394 and USD 0.07677 per task. The first uses parallel drafts with settings chosen by the routing rule. The second uses parallel drafts at medium reasoning effort and a 4,096-token cap. A family CSV has candidate IDs and a `family` or `topology` column. The synthetic fixture supplies those IDs and its `topology` column.

The calibration example gives an optimistic minimum of USD 0.03722412 and a conservative cost of USD 0.05581493 per task. Their difference bounds excess cost on simultaneous coverage. If only the optimistic comparison is feasible, the available data leave feasibility unresolved.

```sh
# Apply all service limits to each nonempty group in each scenario.
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv --scope group

# Bound the paired defect score at the prespecified tolerance.
cargo run --offline -- cost fixtures/calibration-tallies.csv fixtures/scenario-weights.csv \
  --input calibration --method bernstein --epsilon 0.10

# Test an upper bound of five seconds on accumulated queue delay.
cargo run --offline -- cost fixtures/finite-outcomes.csv fixtures/scenario-weights.csv \
  --input outcomes --timing queue --delay 5

# Count acceptances only when the workflow finishes by the deadline.
cargo run --offline -- cost fixtures/finite-outcomes.csv fixtures/scenario-weights.csv \
  --input outcomes --timing timely

# Solve the single-scenario fixed-budget case in one LP.
cargo run --offline -- risk-one-lp fixtures/appendix-adverse-moments.csv fixtures/appendix-adverse-weights.csv \
  --budget 1.3 --completion 0.6 --lateness 0.08
```

Install the command-line interface (CLI) onto your Cargo binary path with `cargo install --path . --locked --offline`.
Use `risk` to minimise the highest defect fraction among accepted outputs across scenarios at a fixed budget. Give `price` a comma-separated `--seed` of candidate IDs. With `--failure`, pricing uses a separate service-slack programme to expand an infeasible seed before minimising cost. The reported pricing gap applies to the supplied finite pool with its coefficients held fixed.

The CLI retains the original acceptance, conditional defect and lateness limits by default. For the task-failure comparisons, pass `none` to `--completion`, `--epsilon` or `--lateness` to omit that limit. Omitting the acceptance floor also requires omitting the conditional defect limit.

| Exit code | Meaning |
| --- | --- |
| 1 | Input validation failed |
| 1 | Numerical solve failed |
| 2 | Infeasible programme |

## Use as a library

Add the unpacked package to your project's `Cargo.toml`.

```toml
[dependencies]
llm-workflow-optimiser = { path = "../llm-workflow-optimiser" }
```

```rust
use llm_workflow_optimiser::{
    io::load_moments, RoutingRestriction, SelectionRequirements, ServiceScope,
};

fn main() -> Result<(), llm_workflow_optimiser::Error> {
    let study = load_moments(
        "fixtures/population-moments.csv",
        "fixtures/scenario-weights.csv",
    )?;
    let limits = SelectionRequirements {
        max_failure: Some(0.30),
        max_defect: Some(0.10),
        completion: Some(0.80),
        max_lateness: Some(0.15),
        scope: ServiceScope::Scenario,
    };
    let rule = study.solve_selection(limits, &RoutingRestriction::Unrestricted, true)?;
    println!("worst-case USD per attempt {:.6}", rule.worst_cost);
    for ((group, weights), rows) in study.groups().iter().zip(&rule.weights).zip(&rule.groups) {
        for (candidate, weight) in study.candidates().iter().zip(weights) {
            if *weight > 0.0 {
                println!("{group}\t{candidate}\t{weight:.8}");
            }
        }
        for (scenario, row) in study.scenarios().iter().zip(rows) {
            println!("{group}\t{scenario}\tacceptance={:.4}", row.acceptance);
        }
    }
    Ok(())
}
```

Use `Study::new` to pass data directly. Index group weights by scenario and then by group. The flat cell array is ordered by `(group, candidate, scenario)`, with scenario varying fastest.
`graph::Workflow::freeze` resolves the request settings from a caller-supplied registry, including model-specific reasoning effort. The frozen workflow's ID identifies the study candidate. Every configuration change requires a new measurement ID, including a revised prompt.

The graph validator accepts directed acyclic graphs. Represent a bounded loop by unfolding its possible calls. Measured outcomes from a cyclic controller can enter the optimiser by candidate ID once termination and finite expected cost have been established for that implementation. The calling platform executes the workflow.
Measure outcomes over complete workflow executions using the fields below. Pass the observed group and a fresh uniform draw in `[0, 1)` to `Policy::select` to obtain the candidate index.

Use `Study::with_eligibility` to restrict configurations by observed group. Use `Study::with_assessed_groups` and `ServiceScope::Assessed` when a service limit applies to independently assessed groups such as incident severity. Routing still uses the group observed at intake.

When shared capacity or state makes outcomes depend on the routing rule, compare complete deployment designs with `deployment::DeploymentStudy`. Measure each design separately through independent repetitions of the full workload. Supply the per-task outcome means or their simultaneous bounds for each scenario. This comparison uses measured outcomes of the scheduler or fallback as part of the design.

## Supply a study

All formats use the same scenario-weight file, with columns `scenario,group,weight`. Every group needs a row within each scenario, including an explicit zero when absent. Each scenario's weights sum to one.

| Input | Schema |
| --- | --- |
| `moments` | `group,scenario,candidate,acceptance,joint_defect,cost_usd,lateness`, with optional `failure`. Defects count accepted results with a material defect. Failure selection requires the failure coefficient. |
| `calibration` | `group,scenario,candidate,attempts,accepted,accepted_defects,late,total_cost,cost_cap`. The cap is a justified upper bound per attempt. Zero attempts with all other fields zero denotes known immediate rejection. |
| `failure-calibration` | The calibration columns plus `failed`, the count of complete failures over all attempts. Two-sided Hoeffding calibration uses ten one-sided endpoints per uncertain cell. The zero-attempt immediate-rejection row has known failure probability one. |
| `outcomes` | `group,scenario,candidate,probability,accepted,defective,cost_usd,duration_seconds,queued`. Each cell's terminal probabilities sum to one. Valid Boolean literals form the set `{0, 1, false, true}`. Set `queued=false` when added queuing cannot apply. |

The loader reads UTF-8 CSV with named columns and supports quoted fields within a line. Every scenario needs a cell for each candidate within every task group. Extra columns are allowed. See [fixture provenance](fixtures/README.md) for the supplied inputs.

The defect constraint is `joint_defect - epsilon * acceptance <= 0`, with a strictly positive acceptance requirement.
Use `ServiceReport.risk_score` to assess the constraint when the input uses a paired score bound or when queue bounds enforce timely acceptance. The field reports an upper bound on the score itself. A ratio formed from separate bounds can give a looser estimate of the conditional defect fraction. If that ratio exceeds one, the budget search caps it at one because a defect probability lies in `[0, 1]`.
Conditional defect budget search takes point coefficients or separate confidence bounds. The single-LP Charnes–Cooper method requires service limits for each scenario as a whole.

## Scope of the results

The library selects routing probabilities from a finite candidate pool. Dominance screening removes configurations that have a suitable replacement. Reduced costs indicate which omitted configurations may improve a restricted solution.
Freeze the candidate set before drawing independent, representative samples within each cell. Simultaneous confidence bounds from those samples constrain the selected rule's service outcomes. For a paired score bound, fix the method and defect tolerance in advance. The data in `fixtures/` describe a synthetic experiment.

The optional queue bounds must hold throughout the permitted load range, with acceptance and defect status unaffected by delay. The exceedance input bounds the probability that an attempt exceeds the delay cap within a cell. The resulting failure coefficient is an upper bound.

The solver uses a dense two-phase simplex implementation for reasonably scaled finite studies, with automatic cost-row scaling. Floating-point results are subject to solver tolerances. The solver reports a numerical error if a pivot is ambiguous or it reaches a solver limit.
Use the bisection interval as a numerical bracket. Allow a margin for numerical uncertainty near a hard service limit.
