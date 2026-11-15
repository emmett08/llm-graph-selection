# LLM Workflow Optimiser

A Rust library and command-line tool implementing Emmett Miller's [*Selecting LLM workflow graphs and reasoning settings under service constraints*](paper/llm-graph-selection-paper.pdf), revised 15 November 2026.

The policy assigns probabilities to complete workflows for each task group. It uses the same probabilities in every scenario. At a fixed defect limit the optimiser minimises worst-scenario mean cost per attempt. It also supports the fixed-budget problem, which minimises worst-case accepted-output defect risk.

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

Use Rust 1.85 or later. The crate has zero external dependencies and builds offline once Rust is installed.

```sh
cargo test --offline
cargo run --offline -- --help
cargo run --offline -- cost fixtures/population-moments.csv fixtures/scenario-weights.csv --prune
```

The last command should report `worst_cost=0.038798338325` (USD per attempt) and 20 candidate-group columns. The results include service by scenario and by group, and the nonzero routing weights.

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

Install the CLI onto your Cargo binary path with `cargo install --path . --locked --offline`. Use `risk` for multiple scenarios. Use `price` with a feasible `--seed` of comma-separated candidate IDs to price the remaining supplied pool. The reported gap concerns that pool and its fixed coefficients. CLI exit code 2 indicates infeasibility of the supplied programme. Input and numerical errors return exit code 1.

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

Use `Study::new` to pass data directly. Group weights are indexed by scenario then group. The flat cell order is group, candidate, scenario, with scenario varying fastest. `graph::Workflow::freeze` resolves model-specific effort and request settings from a caller-supplied registry. Use each frozen workflow's ID as the study candidate ID. Record new measurements under a new ID when prompts, model settings or execution rules change.

Graph validation accepts directed acyclic graphs, including bounded retries expanded into separate nodes. The calling platform supplies provider calls, guard implementations and scheduling. Obtain acceptance, accepted defects, cost and elapsed time from complete workflow executions. Pass a fresh uniform draw in `[0, 1)` and the observed group index to `Policy::select` to obtain the candidate index.

## Supply a study

All formats use the same scenario-weight file, with columns `scenario,group,weight`. Supply one row for each scenario and group, including explicit zeros. Each scenario's weights sum to one.

| Input | Required columns and meaning |
| --- | --- |
| `moments` | `group,scenario,candidate,acceptance,joint_defect,cost_usd,lateness`. Mean per-attempt cost includes rejected executions. Defects are counted jointly with acceptance. |
| `calibration` | `group,scenario,candidate,attempts,accepted,accepted_defects,late,total_cost,cost_cap`. The cap is a justified upper bound per attempt. Zero attempts with all other fields zero denotes known immediate rejection. |
| `outcomes` | `group,scenario,candidate,probability,accepted,defective,cost_usd,duration_seconds,queued`. Each cell's terminal probabilities sum to one. Boolean fields accept `0`, `1`, `false` and `true`. Set `queued=false` when added queuing cannot apply. |

The loader reads UTF-8 CSV with named columns and supports quoted fields within a line. Each distinct group, scenario and candidate combination requires a cell. Extra columns are allowed. See [fixture provenance](fixtures/README.md) for the supplied inputs.

The decision row is `joint_defect - epsilon * acceptance <= 0` with a strictly positive completion floor. With paired calibration or timely queue bounds, use `ServiceReport.risk_score` for the constraint. Its acceptance and joint-defect fields are separate bounds. Their ratio can be uninformative. With separate endpoints, a conservative ratio can exceed one. The budget search caps it at the structural bound of one. Use point moments or separate endpoints for budget search, and point moments or separate endpoints under scenario-wide service for the one-LP case.

## Scope of the results

The library implements finite-pool selection, dominance screening, reduced-cost pricing, and calibration from representative data. The optional queue envelopes require a bound valid throughout the permitted load range. For the probabilistic queue envelope, the exceedance input bounds the chance of exceeding the delay within the entire cell. Calibration requires a frozen candidate set and independent representative draws within each cell. Fix the tolerance and calibration method before a paired analysis. The data in `fixtures/` are synthetic and do not measure provider quality.

The solver uses a dense two-phase simplex implementation for reasonably scaled finite studies. Cost rows are scaled automatically. Feasibility, bisection and pricing results are floating-point results subject to solver tolerances. Solver limits and ambiguous pivots produce a numerical error. Use the lower and upper bisection endpoints as numerical brackets, with appropriate engineering margin near hard service limits.
