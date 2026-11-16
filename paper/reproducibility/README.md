# Reproduce the workflow-selection illustration

This directory contains the finite synthetic experiment underlying the paper. Its outputs also supply the presentation figures. After dependency installation, the experiment runs locally from the supplied files.
Provider documentation supplies valid request settings and token prices. The simulator defines how calls produce outcomes and how it computes token use and elapsed time.

Run the following commands from this directory.

```sh
python -m pip install -r requirements.txt
python simulate_workflows.py
python verify_exact_example.py
python refine_selection.py
python fractional_check.py
```

The main sample contains outcome counts equivalent to 20,000 independent synthetic trials per cell. A cell represents one workflow for one task group in one scenario. The four immediate-rejection cells have known coefficients. The confidence bounds cover the other 216 cells, whose counts represent 4.32 million trials.
Use `--calibration-n 10000` to change the number of trials per cell in the main sample. The sample-size illustration keeps the prespecified seed for each size.

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
| `calibration-counts.csv` | Nonzero multinomial counts for the main sample. Join to `finite-outcomes.csv` on (group, candidate, scenario, outcome). |
| `calibration-moments.csv` | Primary sample estimates with simultaneous one-sided bounds |
| `frontier.csv` | Objective values at each sampled defect threshold, including explicit feasibility flags |
| `routing-plot-data.csv` | Nonzero weights of the exact full-pool solution |
| `calibration-size.csv` | Population outcomes for feasible rules selected from one separately seeded draw per sample size |
| `search-trace.csv` | Restricted-pool objective at each solve, identifying any added candidate with its reduced cost |
| `simulation-results.json` | Numerical summary with the execution metadata needed for reproduction |
| `refinement-results.json` | Population and calibration comparisons across service definitions and sensitivity settings |

Raw files use zero-based indices.

| Index | Meaning |
| --- | --- |
| Group | Mapping `{0: routine, 1: migration}` |
| Scenario | Mapping `{0: nominal, 1: adverse}` |
| Candidate | Position in `candidate-configurations.json`. |
| Outcome | Row in the cell's enumerated terminal-outcome distribution. |

The files retain outcomes from distinct execution branches even when their values coincide. Within each cell, summing the sparse counts recovers the requested sample size `n`. The files include counts for immediate rejection, whose coefficients are known.

The scripts check that each terminal distribution sums to one and that its outcomes satisfy `Y≤A`. They verify the shared execution rules across providers and check that costs scale with tariffs.
For optimisation, the scripts check constraint residuals and verify that reduced-cost search recovers the optimum of the full linear programme (LP).
The scripts check whether the simultaneous bounds from the main sample contain the known population values. Estimating a coverage rate would require independent repetitions of the sampling procedure.

The recorded optimum is USD 0.0387983383 per attempted task in the adverse scenario. Each service constraint binds there.

| Service outcome | Adverse-scenario value |
| --- | ---: |
| Fraction of attempts accepted | 0.80 |
| Fraction of accepted outputs with a material defect | 0.10 |
| Fraction of attempts exceeding the deadline | 0.15 |

The single-call pool is infeasible at these requirements under the specified response laws.
The 80% minimum acceptance rate applies across each scenario, with every original attempt in the denominator. Acceptance means the workflow releases a result under the service's terminal rule. In a release-note service, the corresponding requirement is a minimum publication rate. Within the migration group, the selected rule immediately rejects 30.39% of attempts.
The baseline includes late outputs in acceptance. The joint on-time and defect-free outcome counts attempts that result in an accepted output by the deadline without a material defect. Its denominator also includes all attempts.
The simulator supplies the true group to the router and assigns zero infrastructure cost to routing and orchestration. In an operational study, a classifier supplies the observed routing group. Independent task labels are needed to assess service for the true groups.

Each point in the sample-size illustration comes from one separately seeded draw. Estimating coverage at a given size would require repeated draws at that size.
`verify_exact_example.py` checks the manuscript's small rational LP example separately.

## Model refinements

Run `python refine_selection.py` to reproduce the additional analyses. The script reuses the finite generator with the seed for the main sample and writes `refinement-results.json`.
It checks dominance screening and compares service limits applied across each scenario with limits applied within each task group. Each calibration method fixed before sampling has its own 95% simultaneous coverage guarantee under the corresponding assumptions.
The population sensitivity analyses remove the immediate-rejection option and require acceptance by the deadline. They also add queue delay and vary the adverse migration share over 59–61% using fixed group coefficients. Another calculation reduces the probability that a single call yields an accepted result with a material defect by 25% in every cell, leaving the other coefficients at their original values. This tests how much the feasibility of single-call routing depends on the draft defect probabilities used in the original simulation.
The serial review sensitivity changes the defect probability after a flagged draft completes repair. Outside the shared blind-spot state, the alternative assigns a 50% probability that a detected defect persists. When the initial review falsely flags a sound draft, it adds a 20% chance of introducing a defect conditional on the baseline repair producing a sound result. The alternative is a synthetic test of the routing decision. A live study would need independently adjudicated draft and repair outcomes to estimate these probabilities.

Run `python fractional_check.py` to check bisection's agreement with the Charnes–Cooper LP result on the appendix's adverse scenario. Exact rational arithmetic supplies a matching lower bound at the optimum.
