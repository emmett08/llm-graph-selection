# LLM Workflow Optimiser

A Rust library and command-line tool implementing Emmett Miller's [*Selecting LLM workflow graphs and reasoning settings under service constraints*](paper/llm-graph-selection-paper.pdf), revised 15 November 2026.

For each task group, the optimiser chooses the probability of selecting each complete workflow. It uses the same routing probabilities in every operating scenario.
At a fixed defect limit, it minimises the highest mean cost per attempt across scenarios while meeting the service requirements. With a fixed budget, it instead minimises the highest fraction of accepted outputs that contain a material defect.

## Repository layout

| Path | Contents |
| --- | --- |
| [`src/`](src/) | Cargo library and command-line tool |
| [`tests/`](tests/) | Rust checks against the paper's results |
| [`fixtures/`](fixtures/) | Self-contained synthetic inputs for the Rust tests and examples |
| [`paper/`](paper/) | Compiled paper, LaTeX, BibTeX, figure assets and build instructions |
| [`paper/reproducibility/`](paper/reproducibility/) | Simulation, calibration data, exact checks and source evidence |

See [the paper build and reproduction instructions](paper/README.md).

## Run locally

Use Rust 1.85 or later. The crate uses only the Rust standard library and builds offline once Rust is installed.

```sh
cargo test --offline
cargo run --offline -- --help
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv --prune
```

The last command should report `worst_cost=0.038798338325` (USD per attempt). Dominance screening leaves ten routing variables per task group, giving 20 in total.
The output shows the nonzero routing probabilities alongside the service outcomes for each scenario and task group.

```sh
# Apply all service limits to each nonempty group in each scenario.
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv --scope group

# Use paired calibration at the prespecified defect tolerance.
cargo run --offline -- cost fixtures/calibration-tallies.csv fixtures/scenario-weights.csv \
  --input calibration --method bernstein --epsilon 0.10

# Test an upper bound of five seconds on accumulated queue delay.
cargo run --offline -- cost fixtures/finite-outcomes.csv fixtures/scenario-weights.csv \
  --input outcomes --timing queue --delay 5

# Count only on-time acceptance and on-time accepted defects.
cargo run --offline -- cost fixtures/finite-outcomes.csv fixtures/scenario-weights.csv \
  --input outcomes --timing timely

# Solve the single-scenario fixed-budget case in one LP.
cargo run --offline -- risk-one-lp fixtures/appendix-adverse-moments.csv fixtures/appendix-adverse-weights.csv \
  --budget 1.3 --completion 0.6 --lateness 0.08
```

Install the command-line interface (CLI) onto your Cargo binary path with `cargo install --path . --locked --offline`.
Use `risk` for fixed-budget optimisation across multiple scenarios. Use `price` with a feasible `--seed` of comma-separated candidate IDs to calculate reduced costs for the remaining candidates in the supplied pool. The reported optimality gap applies to that pool with its coefficients held fixed.
Exit code 2 indicates that the supplied programme is infeasible. Input and numerical errors return exit code 1.

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

Use `Study::new` to pass data directly. Group weights are indexed by scenario, then by group. In the flat cell array, scenario varies fastest within each candidate, and candidate varies within each group.
`graph::Workflow::freeze` resolves model-specific effort and request settings from a caller-supplied registry. Use each frozen workflow's ID as the study candidate ID. A change to a prompt or execution setting defines a new configuration, so store its measurements under a new ID.

Graph validation accepts directed acyclic graphs, including bounded retries expanded into separate nodes. The calling platform executes provider calls according to its guards and scheduling rules.
Measure outcomes over complete workflow executions using the fields below. To route a task, pass a fresh uniform draw in `[0, 1)` and the observed group index to `Policy::select`. It returns the candidate index.

## Supply a study

All formats use the same scenario-weight file, with columns `scenario,group,weight`. Supply one row for each scenario and group, including explicit zeros. Each scenario's weights sum to one.

| Input | Required columns and meaning |
| --- | --- |
| `moments` | `group,scenario,candidate,acceptance,joint_defect,cost_usd,lateness`. Mean per-attempt cost includes rejected executions. Defects are counted jointly with acceptance. |
| `calibration` | `group,scenario,candidate,attempts,accepted,accepted_defects,late,total_cost,cost_cap`. The cap is a justified upper bound per attempt. Zero attempts with all other fields zero denotes known immediate rejection. |
| `outcomes` | `group,scenario,candidate,probability,accepted,defective,cost_usd,duration_seconds,queued`. Each cell's terminal probabilities sum to one. Boolean fields accept `0`, `1`, `false` and `true`. Set `queued=false` when added queuing cannot apply. |

The loader reads UTF-8 CSV with named columns and supports quoted fields within a line. Supply a cell for every candidate in every task group and scenario. Extra columns are allowed. See [fixture provenance](fixtures/README.md) for the supplied inputs.

The defect constraint is `joint_defect - epsilon * acceptance <= 0`, with a strictly positive acceptance requirement.
With paired calibration or timely queue bounds, use `ServiceReport.risk_score` to assess that constraint. It bounds the joint score directly. Dividing the separately bounded joint-defect and acceptance fields can give a looser result.
With separate confidence bounds, this conservative ratio can exceed one. The budget search caps it at one because a defect probability lies in `[0, 1]`.
Budget search accepts point estimates or separate confidence bounds. The single-LP method additionally requires scenario-wide service limits.

## Scope of the results

The library selects routing probabilities from a finite candidate pool. Dominance screening removes candidates that have suitable replacements, and reduced-cost pricing identifies candidates to consider adding to a restricted pool.
Calibration uses representative samples to bound the service outcomes of the selected rule. Freeze the candidate set before collecting independent representative draws within each cell. Fix the tolerance and calibration method before a paired analysis. The data in `fixtures/` describe a synthetic experiment.

The optional queue bounds must hold throughout the permitted load range. For the probabilistic queue bound, the exceedance input bounds the probability that an attempt exceeds the delay cap within that cell.

The solver uses a dense two-phase simplex implementation for reasonably scaled finite studies and scales cost rows automatically. Its results use floating-point arithmetic and are subject to solver tolerances. Reaching a solver limit or encountering an ambiguous pivot produces a numerical error.
Treat the lower and upper bisection endpoints as numerical brackets. Allow an engineering margin when decisions lie near a hard service limit.
