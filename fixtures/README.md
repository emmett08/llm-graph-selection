# Paper fixtures

These inputs reproduce the finite simulator and the rational appendix of *Choosing LLM workflow topologies under cost and failure constraints*. The terminal outcomes and service coefficients come from the simulator. The trial counts are synthetic. All cost fields are USD per attempted task except `total_cost`, which sums the costs of the sampled attempts. Configuration and provider names identify the cases in the paper.

| Input | Source and transformation |
| --- | --- |
| `population-moments.csv` | The 220 cells from `paper/reproducibility/population-moments.csv` with `failure` calculated by summing the probabilities of terminal outcomes without timely, defect-free acceptance. |
| `scenario-weights.csv` | Group proportions from `reproducibility/simulation-results.json`. Each scenario has two group weights summing to one. |
| `finite-outcomes.csv` | `reproducibility/finite-outcomes.csv` with names substituted for numeric IDs. Identical terminal records within each cell are combined by summing probabilities. The immediate rejection route is unqueued. |
| `calibration-tallies.csv` | `paper/reproducibility/calibration-counts.csv` joined to terminal outcomes for the seed `20261114`. The file gives acceptance, accepted-defect, lateness and failure counts from the same attempts, total cost and the cost cap per attempt. The 216 uncertain cells each have 20,000 attempts. Four known immediate-rejection cells use the zero-attempt sentinel with zero sample counts and an exact failure probability of one. |
| `appendix-*.csv` | Four rational appendix candidates, including rejection. The adverse-only pair is used to verify the one-LP fractional transformation. |

The main checks use a 10% defect limit, 80% completion floor, 15% lateness limit and 180-second deadline. The paper's primary comparison retains these limits and caps failure at 30% of all submitted tasks. The appendix uses a 10% defect limit, 60% completion floor and 8% lateness limit. Its adverse-only fixed-budget example uses USD 1.30 per attempt.
