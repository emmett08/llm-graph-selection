# LLM Workflow Optimiser

A Rust implementation of Emmett Miller's [*Selecting LLM workflow configurations under service constraints*](paper/llm-graph-selection-paper.pdf), revised 15 November 2026. The library includes a command-line interface.

For each task group, the optimiser chooses the probability of selecting each complete workflow. It uses the same routing probabilities in every operating scenario.
The optimisation supports two objectives, each subject to the service requirements.

| Fixed input | Quantity minimised across scenarios |
| --- | --- |
| Defect limit | Highest mean cost per attempt |
| Budget | Highest fraction of accepted outputs containing a material defect |

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
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv --prune
```

The last command should report `worst_cost=0.038798338325` (USD per attempt). Dominance screening leaves ten routing variables per task group, giving 20 in total.
The output gives nonzero routing probabilities with service outcomes at scenario level, followed by results within each task group.

```sh
# Apply all service limits to each nonempty group in each scenario.
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv --scope group

# Use paired calibration at the prespecified defect tolerance.
cargo run --offline -- cost fixtures/calibration-tallies.csv fixtures/scenario-weights.csv \
  --input calibration --method bernstein --epsilon 0.10

# Test an upper bound of five seconds on accumulated queue delay.
cargo run --offline -- cost fixtures/finite-outcomes.csv fixtures/scenario-weights.csv \
  --input outcomes --timing queue --delay 5

# Apply the deadline to acceptance before counting accepted defects.
cargo run --offline -- cost fixtures/finite-outcomes.csv fixtures/scenario-weights.csv \
  --input outcomes --timing timely

# Solve the single-scenario fixed-budget case in one LP.
cargo run --offline -- risk-one-lp fixtures/appendix-adverse-moments.csv fixtures/appendix-adverse-weights.csv \
  --budget 1.3 --completion 0.6 --lateness 0.08
```

Install the command-line interface (CLI) onto your Cargo binary path with `cargo install --path . --locked --offline`.
The `risk` command solves fixed-budget optimisation across multiple scenarios. To calculate reduced costs for the remaining candidates, give `price` a feasible `--seed` of comma-separated candidate IDs. The reported optimality gap applies to the supplied pool with its coefficients held fixed.

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
use llm_workflow_optimiser::{io::load_moments, Requirements, ServiceScope};

fn main() -> Result<(), llm_workflow_optimiser::Error> {
    let study = load_moments(
        "fixtures/population-moments.csv",
        "fixtures/scenario-weights.csv",
    )?;
    let limits = Requirements {
        epsilon: 0.10,
        completion: 0.80,
        lateness: 0.15,
        scope: ServiceScope::Group,
    };
    let policy = study.solve(limits, true)?;
    println!("worst-case USD per attempt {:.6}", policy.worst_cost);
    for ((group, weights), rows) in study.groups().iter().zip(&policy.weights).zip(&policy.groups) {
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

Use `Study::new` to pass data directly. Group weights are indexed by scenario, then by group. The flat cell array follows the nesting `(group, candidate, scenario)`, with scenario varying fastest.
`graph::Workflow::freeze` resolves the request settings from a caller-supplied registry, including model-specific reasoning effort. The frozen workflow's ID identifies the study candidate. Every configuration change requires a new measurement ID, including a revised prompt.

Graph validation accepts directed acyclic graphs, including bounded retries expanded into separate nodes. The calling platform schedules provider calls subject to the edge guards.
Measure outcomes over complete workflow executions using the fields below. `Policy::select` returns a candidate index for the observed task group using a fresh uniform draw in `[0, 1)`.

## Supply a study

All formats use the same scenario-weight file, with columns `scenario,group,weight`. Every group needs a row within each scenario, including an explicit zero when absent. Each scenario's weights sum to one.

| Input | Schema |
| --- | --- |
| `moments` | `group,scenario,candidate,acceptance,joint_defect,cost_usd,lateness`. Mean per-attempt cost includes rejected executions. Defects are counted jointly with acceptance. |
| `calibration` | `group,scenario,candidate,attempts,accepted,accepted_defects,late,total_cost,cost_cap`. The cap is a justified upper bound per attempt. Zero attempts with all other fields zero denotes known immediate rejection. |
| `outcomes` | `group,scenario,candidate,probability,accepted,defective,cost_usd,duration_seconds,queued`. Each cell's terminal probabilities sum to one. Valid Boolean literals form the set `{0, 1, false, true}`. Set `queued=false` when added queuing cannot apply. |

The loader reads UTF-8 CSV with named columns, supporting quoted fields within a line. Every scenario needs a cell for each candidate within every task group. Extra columns are allowed. See [fixture provenance](fixtures/README.md) for the supplied inputs.

The defect constraint is `joint_defect - epsilon * acceptance <= 0`, with a strictly positive acceptance requirement.
Use `ServiceReport.risk_score` to assess that constraint under paired calibration. The same requirement applies whenever timely queue bounds supply the coefficients. This field bounds the joint score directly. A ratio formed from separate coefficient bounds can be unnecessarily conservative.
With separate confidence bounds, this conservative ratio can exceed one. The budget search caps it at one because a defect probability lies in `[0, 1]`.
Budget search accepts point coefficients, with separate confidence bounds supported as another input form. The single-LP method additionally requires scenario-wide service limits.

## Scope of the results

The library selects routing probabilities from a finite candidate pool. Dominance screening reduces this pool by identifying suitable replacements for removed candidates. A restricted-pool search uses reduced costs to guide expansion.
Calibration bounds the selected rule's service outcomes using independent representative draws within each cell. Freeze the candidate set before sampling. A paired analysis also requires a prespecified method at a fixed defect tolerance. The data in `fixtures/` describe a synthetic experiment.

The optional queue bounds must hold throughout the permitted load range. For the probabilistic queue bound, the exceedance input bounds the probability that an attempt exceeds the delay cap within that cell.

The solver uses a dense two-phase simplex implementation for reasonably scaled finite studies, with automatic cost-row scaling. Floating-point results are subject to solver tolerances. An ambiguous pivot produces a numerical error, as does reaching a solver limit.
Treat the bisection interval as a numerical bracket. Allow an engineering margin near a hard service limit.
