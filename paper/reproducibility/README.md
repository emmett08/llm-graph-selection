# Reproduce the workflow-selection illustration

This directory contains the finite synthetic experiment used in the paper and slides, together with its numerical outputs. After installing the dependencies, the experiment runs locally from the supplied files.
Provider documentation supplies model compatibility and unit prices. The experiment stipulates the outcome distributions and the rules for token use and duration.

Run from this directory:

```sh
python -m pip install -r requirements.txt
python simulate_workflows.py
python verify_exact_example.py
```

The primary calibration uses outcome counts equivalent to 20,000 independent synthetic trials for each candidate within each task group and scenario. Immediate rejection has known outcomes, leaving 216 cells to calibrate.
Use `--calibration-n 10000` to change the primary calibration sample size. The sample-size illustration has its own fixed sizes and seeds.

The recorded run used Python 3.12.14. Dependency versions are pinned in `requirements.txt` and recorded in `simulation-results.json`.
A typical run takes a few seconds and overwrites the generated data in this directory and the plots in `../figures`. The provider registry supplies the inputs and remains unchanged.
Reproduction should recover the coefficients and objective values with the specified seeds. Floating-point solver tie-breaking and PDF metadata can differ across environments.

`simulation-method.md` explains the simulation rules and the conclusions they support. The executable source is the complete specification.
Each data figure has a source CSV, with vector PDF and 220-dpi PNG versions for reuse.

| File | Contents |
|---|---|
| `model-registry.json` | Dated official model IDs, supported settings, endpoint parameter mappings, prices and source URLs |
| `candidate-configurations.json` | Exact ordered list of 55 graph/model/effort/cap candidates |
| `finite-outcomes.csv` | Finite terminal-outcome distributions, with cell/outcome indices, probabilities and A/Y/C/T/L values |
| `population-moments.csv` | Exact population coefficients and finite cost caps for 220 cells |
| `calibration-counts.csv` | Nonzero multinomial counts for the primary calibration; join on group/candidate/scenario/outcome to `finite-outcomes.csv` |
| `calibration-moments.csv` | Primary sample estimates and simultaneous one-sided bounds |
| `frontier.csv` | All sampled defect thresholds, objective values and explicit feasibility flags |
| `routing-plot-data.csv` | Nonzero weights of the exact full-pool solution |
| `calibration-size.csv` | One separately seeded calibration realisation per sample size, with feasibility and exact evaluation |
| `search-trace.csv` | Restricted-pool objective, added candidate and reduced cost at every solve |
| `simulation-results.json` | Summary values, weights, limits, seeds, software versions and elapsed runtime |

The raw distribution and count files use zero-based indices.

| Index | Meaning |
| --- | --- |
| Group | 0 is routine work and 1 is migration work. |
| Scenario | 0 is nominal and 1 is adverse. |
| Candidate | Position in `candidate-configurations.json`. |
| Outcome | Row in the cell's enumerated terminal-outcome distribution. |

Outcomes with identical values remain separate when they arise from different execution branches. Summing the sparse calibration counts within a cell gives the requested sample size `n`.
The files also contain counts for the four immediate-rejection cells. Their coefficients are exactly known, so the calibration guarantee covers the other 216 cells.

The source checks that each terminal distribution is normalised and satisfies `Y≤A`. It checks that providers share the stipulated quality and timing laws, with costs proportional to their tariffs.
The optimisation checks cover constraint residuals and agreement between reduced-cost search and the full linear programme (LP).
The primary calibration also checks whether its simultaneous bounds cover the known population values. This is a check on one realised sample. Estimating a coverage rate would require repeated calibration samples.

The recorded optimum is USD 0.0387983383 per attempted task in the adverse scenario. Each service constraint binds there.

| Service outcome | Adverse-scenario value |
| --- | ---: |
| Fraction of attempts accepted | 0.80 |
| Fraction of accepted outputs with a material defect | 0.10 |
| Fraction of attempts exceeding the deadline | 0.15 |

The single-call pool is infeasible at these requirements. These results describe the specified finite simulator.
The 80% acceptance requirement applies across each whole scenario. Within the migration group, the selected rule immediately rejects 30.39% of attempts.
The simulator supplies correct group labels at zero cost and assigns zero orchestration overhead.

The primary calibration represents 216 × 20,000 = 4.32 million independent synthetic trials. Each point in the sample-size illustration comes from one separately seeded draw. Replicated draws at each size would be needed to estimate repeated-sampling coverage or average changes with sample size.
`verify_exact_example.py` checks the manuscript's small rational LP example separately.

## Model refinements

Run `python refine_selection.py` to reproduce the additional analyses. It reuses the finite generator and primary calibration seed, then writes `refinement-results.json`.
The selection analyses compare dominance screening and service limits applied within each task group. The calibration comparison gives each method its own prespecified 95% simultaneous guarantee.
The timing analyses evaluate timely completion and sensitivity to accumulated extra queue delay across a workflow. All calculations use the finite simulator.

Run `python fractional_check.py` to compare a single Charnes–Cooper LP with bisection on the adverse scenario of the appendix. The script checks the numerical results against the exact rational optimum and its lower bound.
