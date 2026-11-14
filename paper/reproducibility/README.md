# Reproduce the workflow-selection illustration

This directory accompanies the paper and slides. It contains a complete finite synthetic experiment and its numerical outputs. No API key, LLM call or network connection is required. Only model compatibility and unit prices are drawn from provider documentation; every response, usage and duration law is a stated assumption.

Run from this directory:

```sh
python -m pip install -r requirements.txt
python simulate_workflows.py
python verify_exact_example.py
```

The default primary calibration uses 20,000 synthetic trials per nontrivial group–candidate–scenario cell. `--calibration-n 10000` changes that primary calibration only; it does not change the separate sample-size illustration. Python 3.12.14 was used. NumPy, SciPy and Matplotlib versions are pinned below and recorded in `simulation-results.json`. A typical run takes a few seconds. The generated outputs overwrite files in this directory and `../figures`; the provider registry is read only. Floating-point solver tie-breaking and PDF metadata can differ across environments. The coefficients, objective values and specified seeds are the reproducibility target.

Read `simulation-method.md` for every constant, branch, cost rule and inference limit. The executable source remains the canonical complete specification. The figure data are CSV files, and vector PDF and 220-dpi PNG versions are included.

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

Indices in the raw distribution/count files are zero-based: group 0=routine, 1=migration; scenario 0=nominal, 1=adverse; candidate index is its position in `candidate-configurations.json`. Outcomes are rows of each cell's enumerated distribution. Duplicate-valued outcomes are retained because they arise from separate branches. Summing the sparse calibration counts per cell gives the requested n, including the exactly known reject cells, whose counts are generated but do not consume the 216-cell calibration guarantee.

The source performs checks for normalised probabilities, Y≤A, provider-exchangeable quality and timing, correct relative prices, LP feasibility residuals, and agreement of column search with the full LP. The recorded primary calibration also checks its simultaneous bounds against the known population; this realised success does not establish an empirical 95% coverage rate.

The recorded optimum is USD 0.0387983383 per attempted task in the adverse scenario. At that solution, adverse acceptance is 0.80, accepted defect risk is 0.10 and lateness probability is 0.15. The single-call pool is infeasible at those requirements. These are properties of the specified finite simulator and do not estimate provider performance. The 80% acceptance requirement applies to each whole scenario, not each group: the selected migration rule immediately rejects 30.39% of attempted tasks. Observed group labels are supplied without error or classification cost; classification and orchestration overhead are zero. The primary calibration uses 216 uncertain cells × 20,000 trials = 4.32 million synthetic trials. Each sample-size point is a separate single draw and does not estimate convergence or repeated-sampling coverage. `verify_exact_example.py` separately checks the manuscript's small rational LP example.

## Model refinements

Run `python refine_selection.py` to reproduce dominance screening, group-level service requirements, paired calibration, queue-delay sensitivity and timely completion. It reuses the finite generator and the primary calibration seed and writes `refinement-results.json`. Each calibration method uses its own prespecified 95% simultaneous guarantee. The queue-delay values are sensitivity parameters for the accumulated extra delay across a workflow. The script makes no provider calls.

Run `python fractional_check.py` to compare a single Charnes–Cooper LP and bisection on the adverse scenario of the appendix. The script also verifies the exact rational result and certificate.
