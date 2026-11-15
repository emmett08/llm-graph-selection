# Paper fixtures

These inputs reproduce the finite simulator and the rational appendix of *Selecting LLM workflow graphs and reasoning settings*, revision of 14 November 2026. The terminal outcomes and service coefficients come from the simulator. The trial counts are synthetic. All cost fields are USD per attempted task except `total_cost`, which sums the costs of the sampled attempts. Configuration and provider names identify the cases in the paper.

| Input | Source and transformation |
| --- | --- |
| `population-moments.csv` | Original `reproducibility/population-moments.csv` with 220 cells. |
| `scenario-weights.csv` | Group proportions from `reproducibility/simulation-results.json`. Each scenario has two group weights summing to one. |
| `finite-outcomes.csv` | `reproducibility/finite-outcomes.csv` with names substituted for numeric IDs. Identical terminal records within each cell are combined by summing probabilities. The immediate rejection route is unqueued. |
| `calibration-tallies.csv` | `reproducibility/calibration-counts.csv` joined to terminal outcomes for the seed `20261114`. The file counts acceptances and records how many accepted outputs contain a defect. It counts late attempts and gives the total charge with the cost cap per attempt. The 216 uncertain cells each have 20,000 attempts. Four known zero-outcome immediate rejection cells use the zero-attempt sentinel. |
| `appendix-*.csv` | Four rational appendix candidates, including rejection. The adverse-only pair is used to verify the one-LP fractional transformation. |

The main checks use a 10% defect limit, 80% completion floor, 15% lateness limit and 180-second deadline. The appendix uses a 10% defect limit, 60% completion floor and 8% lateness limit. Its adverse-only fixed-budget example uses USD 1.30 per attempt.
