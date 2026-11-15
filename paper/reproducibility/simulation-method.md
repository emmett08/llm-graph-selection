# Simulation specification

This finite synthetic experiment evaluates stipulated execution laws. Provider documentation supplies admissible model settings and input/output prices.
Every provider uses the same outcome distributions and the same rules for token use and duration. Their token prices differ, so provider selection reflects the tariff comparison within this model. Measuring provider performance would require observations from complete workflow executions.

## Candidate domain and evaluation target

The pool combines three models with three graph families. Each combination has three effort settings and two output caps, giving 54 configured workflows. Immediate rejection is the 55th candidate.

| Configuration dimension | Values |
| --- | --- |
| Model | `gpt-6.1-sol`, `claude-sonnet-5-5`, `gemini-4-hydrogen` |
| Graph | Single call, serial review with repair, parallel drafts with a merge |
| Effort | `low`, `medium`, `high` |
| Generated-token cap per call | 4,096 or 8,192 |

Every call within a configured graph uses the chosen model with the same effort setting and cap. The experiment covers this restricted part of the mathematical model, which permits settings to vary between nodes. The sweep follows the configuration dimensions in the table. Changes to source-access edges or prompts would require an additional experiment.

The router observes the task group g, which is routine (0) or migration (1). The simulator supplies correct labels and assigns zero cost to classification and orchestration.
The scenario k determines operating conditions and task proportions. Routine/migration proportions are (0.70, 0.30) in the nominal scenario (0) and (0.40, 0.60) in the adverse scenario (1). Routing depends on the observed group alone, so the same group-specific probabilities apply in both scenarios.

The simulator records the following outcomes for each attempt.

| Symbol | Outcome |
| --- | --- |
| A | Acceptance indicator |
| Y | Acceptance with a material defect, so Y≤A |
| C | Total charge in USD |
| T | Elapsed time in seconds |
| L | Lateness indicator, L=1{T>180} |

The acceptance rule permits late outputs. Rejected attempts retain their incurred cost and duration. Immediate rejection has A=Y=C=T=L=0.
In each scenario, acceptance must be at least 0.80 and the fraction of accepted outputs with a material defect at most 0.10. At most 0.15 of all attempts may exceed the deadline. The objective minimises the highest expected cost per attempt across scenarios.

## Shared latent state

Each attempt draws difficulty D with probability 0.15+0.40g+0.10k. Conditional on D, it draws a blind spot B with probability 0.01+0.02D+0.01k.
The slow-load indicator S has probability 0.06+0.08k and is independent of (D,B) within each scenario. It multiplies call duration by 1+0.7S.
All calls in the graph share the state (D,B,S). Subsequent error and review draws are conditionally independent given this state and their parent outcomes under the graph rules. The shared state therefore allows dependence between parallel drafts and between repeated checks. Distinct attempted tasks are independent within each cell.

## Synthetic usage and duration

| Role | Base input tokens | Desired generated tokens at low / medium / high | Extra generated tokens if D=1 |
|---|---:|---:|---:|
| Draft | 2,200 | 900 / 2,200 / 5,000 | 1,400 |
| Review | 3,600 | 500 / 1,400 / 3,600 | 800 |
| Repair | 4,200 | 1,000 / 2,500 / 5,500 | 1,200 |
| Merge | 5,000 | 700 / 1,800 / 4,200 | 1,000 |

Input tokens equal the base plus 1,000D. Let O be the desired generated count and H the selected cap. The billed generated count is min(O,H), including reasoning tokens once alongside visible answer tokens.
The simulator defines completion by O≤H. A truncated call causes rejection under the graph rules below. This cap-to-failure relation is stipulated for the experiment.
The billing model charges short-context, uncached input tokens at the registry's standard rate. It includes the model-call charges defined here, with zero orchestration charges.

For input count I and billed generated count U, call cost is (p_in I+p_out U)/10^6 USD. Rates p_in/p_out are 2/10 for Sol and Sonnet and 1.5/7.5 for Hydrogen.
The stipulated call duration is (1+0.7S)(2+0.008I+0.012U) seconds. Costs add over executed calls, and durations add over sequential stages. The parallel draft stage lasts as long as its slower draft. Here both draft durations are equal because they share their settings and latent state.
The simulator holds execution laws fixed across routing mixtures, so the coefficients remain valid as routing probabilities change.

## Error and check laws

For effort e=low/medium/high, define multipliers f_e=(1,0.62,0.35) and detection bases d_e=(0.65,0.82,0.94). The draft defect probability is p=1 when B=1, and p=(0.12+0.26D+0.06k)f_e otherwise.
A check flags a defective output with probability d=0.08 when B=1, and d=d_e−0.08D otherwise. It flags a correct output with probability 0.03.
The simulator retains the result's latent defect status independently of the check verdict. An accepted output with an undetected defect therefore contributes to Y.

The single graph calls one draft and accepts if it completes.

The serial graph calls a draft, then review. Truncation of either rejects. If review passes, it accepts the original draft. If review flags, it calls repair and then a second review. Repair is defective with probability one when B=1 and 0.65p otherwise. A truncated repair rejects. A completed repair is accepted only if the second review passes. There is at most one repair, and the second review uses the same role/cap and check law as the first. Thus this branch is a finite acyclic graph with up to four calls.

The parallel graph calls two drafts concurrently, then merges them. Truncation of either draft or the merge causes rejection. Conditional on the common state, draft defects are independent Bernoulli(p).
The merge defect probability is one when B=1. When B=0, it depends on how many drafts are defective.

| Defective drafts | Merge defect probability when B=0 |
| --- | --- |
| Both | (0.85,0.70,0.55)_e |
| One | (0.25,0.14,0.07)_e+0.04D |
| Neither | 0.01+0.01D |

The merge applies the check law to its result within the same call and accepts only on a pass. The model charges for this combined operation as one call. Every executed draft incurs its cost, including when the workflow rejects the result.

## Exact population calculation

The script enumerates the shared state (D,B,S) and every subsequent correctness event and review decision. The resulting finite terminal distribution has rows of the form (probability, A, Y, C, T, L).
Probability-weighted sums give the population coefficients (a,b,c,ℓ)=(E[A],E[Y],E[C],E[L]) for every candidate within each task group and scenario. These coefficients are exact up to floating-point arithmetic.
The objective frontier and population-optimal routing mix use these enumerated coefficients. They also supply the population evaluation of each calibrated rule.
`finite-outcomes.csv` records each terminal row, including identical values reached through distinct paths. `population-moments.csv` records the resulting coefficients.

The linear programme (LP) uses 2×55 routing variables and a variable z that bounds expected cost in every scenario. Two equalities make routing probabilities sum to one within each task group.
Each scenario contributes four inequalities.

| Requirement | Inequality |
| --- | --- |
| Cost upper bound | c−z≤0 |
| Defect limit | b−0.10a≤0 |
| Acceptance floor | −a≤−0.80 |
| Lateness ceiling | ℓ≤0.15 |

SciPy's HiGHS solver computes the solution. The code requires the maximum inequality residual to be below 10^−8.
It checks that every probability distribution sums to one and satisfies Y≤A. It also checks the shared outcome laws across providers and Hydrogen's cost ratio of 0.75 relative to the other providers.

## Frozen-pool calibration

Calibration treats the coefficients of N=54×2×2=216 cells as unknown. Immediate rejection has exactly known coefficients.
For each uncertain cell, one multinomial draw of n terminal outcomes is equivalent to n independent attempts from the enumerated population distribution. Trials are independent between cells.
At family-wise error α=0.05, the simultaneous one-sided Hoeffding bounds use radius r=sqrt(log(4N/α)/(2n)).

| Quantity | Bound |
| --- | --- |
| Acceptance probability | Lower bound max(0,â−r) |
| Joint probability of acceptance with a material defect | Upper bound min(1,b̂+r) |
| Lateness probability | Upper bound min(1,ℓ̂+r) |
| Expected cost | Upper bound min(Cmax,ĉ+Cmax r) |

Cmax is the maximum possible cost in that cell's finite execution distribution. For deployment, establish the cost cap from enforceable execution and charging limits.

The primary run uses n=20,000 per cell and seed 20261114. The plotted sample-size illustration uses n in {1,000,3,000,10,000,30,000,100,000}, with seed 20261113+n for each n.
Each size has one separately seeded realisation. The 95% simultaneous coverage guarantee applies to the fixed family of bounds within each experiment. A joint guarantee across sample sizes would require an error allocation covering all five experiments.
The sample sizes and candidate domain are fixed before calibration. After computing the bounds, the LP selects a routing rule. Enumeration then supplies that rule's population cost and defect risk.

## Known-coefficient column search

The restricted set starts with 18 single-call candidates and immediate rejection. A fixed Hydrogen serial-review candidate with high effort and an 8,192-token cap makes the starting pool feasible, giving 20 candidates.
The LP dual solution determines the reduced cost of every omitted candidate for each group using its known population coefficients. The candidate with the most negative reduced cost is added for both groups.
Search stops when all omitted reduced costs exceed −10^−10. After adding three candidates, the fourth solve matches the full-pool optimum within 10^−10 USD per attempt. The final pool contains 23 candidates.
Each iteration inspects the entire omitted pool. Establishing savings in evaluation cost would require a procedure that ranks or screens candidates before collecting their outcome data. A learned predictor could be assessed in that experiment.

The frontier solves 69 defect thresholds from 0.03 to 0.20 in increments of 0.0025, with the other constraints fixed. It compares the whole finite pool with the single-call pool plus rejection.
Unplotted grid points are infeasible. Locating the feasibility threshold between adjacent points would require a finer search.

## Scope of the results

The simulator derives outcome coefficients from complete workflows. The coefficients incorporate dependence from shared conditions and the consequences of conditional execution. This allows the linear mixture selector to account for repair paths and rejection at a token cap. Duration calculations account for parallel work.
The calibration experiments show how conservative bounds can make the programme infeasible at small sample sizes and raise its objective when feasible.
The numerical optimum applies to these 55 candidates under the stipulated laws. Deployment conclusions require measured workflow outcomes under the intended load. Assess a predictor or a proposal policy based on provider advice using held-out workloads and an explicit evaluation budget.
