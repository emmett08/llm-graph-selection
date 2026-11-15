# Reproduce the workflow-selection illustration

This directory contains the finite synthetic experiment underlying the paper. Its outputs also supply the presentation figures. After dependency installation, the experiment runs locally from the supplied files.
Provider documentation establishes model compatibility, including the unit prices used to bill calls. The experiment stipulates the complete execution laws from outcome generation through token use to duration.

Run the following commands from this directory.

```sh
python -m pip install -r requirements.txt
python simulate_workflows.py
python verify_exact_example.py
```

The primary calibration uses outcome counts equivalent to 20,000 independent synthetic trials for each candidate within a task group in each scenario. Immediate rejection has known outcomes, leaving 216 cells to calibrate.
Use `--calibration-n 10000` to change the primary calibration sample size. The sample-size illustration retains its prespecified seed at each fixed size.

The recorded run used Python 3.12.14. Install the dependency versions pinned in `requirements.txt`. Each run records its versions in `simulation-results.json`.
A typical run takes a few seconds, overwriting the generated data here before updating the plots in `../figures`. The provider registry remains an unchanged input.
Reproduction should recover the objective values from matching coefficients under the specified seeds. Environment-dependent solver tie-breaking can alter the chosen optimiser. PDF metadata can also vary.

`simulation-method.md` explains the conclusions supported by the simulation rules. The executable source is the complete specification.
Each data figure comes from a source CSV. Reuse formats include a vector PDF alongside a 220-dpi PNG.

| File | Contents |
|---|---|
| `model-registry.json` | Dated model registry with endpoint compatibility, priced from linked documentation |
| `candidate-configurations.json` | Exact ordered list of 55 graph/model/effort/cap candidates |
| `finite-outcomes.csv` | Terminal-outcome distributions indexed by cell, with probability assigned to each A/Y/C/T/L row |
| `population-moments.csv` | Population coefficients for 220 cells, including finite cost caps |
| `calibration-counts.csv` | Nonzero multinomial counts for the primary calibration. Join to `finite-outcomes.csv` on (group, candidate, scenario, outcome). |
| `calibration-moments.csv` | Primary sample estimates with simultaneous one-sided bounds |
| `frontier.csv` | Objective values at each sampled defect threshold, including explicit feasibility flags |
| `routing-plot-data.csv` | Nonzero weights of the exact full-pool solution |
| `calibration-size.csv` | One separately seeded calibration realisation per sample size, evaluated for population outcomes subject to feasibility |
| `search-trace.csv` | Restricted-pool objective at each solve, identifying any added candidate with its reduced cost |
| `simulation-results.json` | Numerical summary with the execution metadata needed for reproduction |

Raw files use zero-based indices.

| Index | Meaning |
| --- | --- |
| Group | Mapping `{0: routine, 1: migration}` |
| Scenario | Mapping `{0: nominal, 1: adverse}` |
| Candidate | Position in `candidate-configurations.json`. |
| Outcome | Row in the cell's enumerated terminal-outcome distribution. |

Outcomes with identical values remain separate when they arise from different execution branches. Summing the sparse calibration counts within a cell gives the requested sample size `n`.
The files also contain counts for the four immediate-rejection cells. Their coefficients are exactly known, so the calibration guarantee covers the other 216 cells.

Each terminal distribution is checked for normalisation subject to `Y≤A`. Provider-level checks verify the stipulated common execution laws, with costs proportional to tariffs.
Optimisation checks examine constraint residuals before verifying that reduced-cost search recovers the full linear programme's (LP) optimum.
The primary calibration also checks whether its simultaneous bounds cover the known population values. This is a check on one realised sample. Estimating a coverage rate would require repeated calibration samples.

The recorded optimum is USD 0.0387983383 per attempted task in the adverse scenario. Each service constraint binds there.

| Service outcome | Adverse-scenario value |
| --- | ---: |
| Fraction of attempts accepted | 0.80 |
| Fraction of accepted outputs with a material defect | 0.10 |
| Fraction of attempts exceeding the deadline | 0.15 |

The single-call pool is infeasible at these requirements. These results describe the specified finite simulator.
The 80% acceptance requirement applies across each whole scenario. Within the migration group, the selected rule immediately rejects 30.39% of attempts.
The simulator supplies correct group labels at zero infrastructure cost, including orchestration overhead.

The primary calibration represents 216 × 20,000 = 4.32 million independent synthetic trials. Each point in the sample-size illustration comes from one separately seeded draw. Repeated-sampling estimates require replicated draws at each size, including when assessing coverage across sample sizes.
`verify_exact_example.py` checks the manuscript's small rational LP example separately.

## Model refinements

Run `python refine_selection.py` to reproduce the additional analyses. It reuses the finite generator with the primary calibration seed before writing `refinement-results.json`.
Selection analyses assess dominance screening, then apply service limits within each task group. Every calibration method receives its own prespecified 95% simultaneous guarantee.
The timing calculations examine sensitivity to accumulated queue delay. Acceptance by the deadline receives a further analysis. All calculations use the finite simulator.

Run `python fractional_check.py` to check bisection's agreement with the Charnes–Cooper LP result on the appendix's adverse scenario. Exact rational arithmetic supplies a matching lower bound at the optimum.
