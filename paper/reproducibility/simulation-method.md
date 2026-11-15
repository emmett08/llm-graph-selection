# Simulation specification

This finite synthetic experiment evaluates stipulated execution laws under provider-documented model settings, using published token prices for billing.
Every provider uses identical laws over complete executions. The resulting selection depends on the tariff assigned to each provider. Measuring provider performance would require observations from complete workflow executions.

## Candidate domain

The pool combines three models with three graph families. Sweeping three effort settings at each of two output caps gives 54 configured workflows. Immediate rejection is the 55th candidate.

| Configuration dimension | Values |
| --- | --- |
| Model | `gpt-6.1-sol`, `claude-sonnet-5-5`, `gemini-4-hydrogen` |
| Graph | Single call, serial review with repair, parallel drafts with a merge |
| Effort | `low`, `medium`, `high` |
| Generated-token cap per call | {4,096, 8,192} |

Every call within a configured graph uses identical request settings on the chosen model. The experiment covers this restricted part of the mathematical model, following the dimensions in the table. A broader experiment could assess node-specific settings, extending to edits of source-access edges through to changes in prompts.

The router observes task group g under the mapping `{0: routine, 1: migration}`. The simulator supplies correct labels at zero routing-infrastructure cost, including orchestration.
Scenario k defines operating conditions for a specified task distribution.

| Scenario k | Conditions | Routine proportion | Migration proportion |
| --- | --- | ---: | ---: |
| 0 | Nominal | 0.70 | 0.30 |
| 1 | Adverse | 0.40 | 0.60 |

Routing depends on the observed group alone, so the same group-specific probabilities apply in every scenario.

The simulator records the following outcomes for each attempt.

| Symbol | Outcome |
| --- | --- |
| A | Acceptance indicator |
| Y | Acceptance with a material defect, so Y≤A |
| C | Total charge in USD |
| T | Elapsed time in seconds |
| L | Lateness indicator, L=1{T>180} |

The acceptance rule permits late outputs. Every rejected attempt retains its measurements (C,T). Immediate rejection has A=Y=C=T=L=0.
In each scenario, the acceptance floor is 0.80, subject to a material-defect ceiling of 0.10 among accepted outputs. At most 0.15 of all attempts may exceed the deadline. The objective minimises the highest expected cost per attempt across scenarios.

## Shared latent state

Each attempt draws difficulty D with probability 0.15+0.40g+0.10k. Conditional on D, it draws a blind spot B with probability 0.01+0.02D+0.01k.
The slow-load indicator S has probability 0.06+0.08k, independently of (D,B) within each scenario. It multiplies call duration by 1+0.7S.
All calls in the graph share the state (D,B,S). Subsequent random draws are conditionally independent given this state, subject to their parent outcomes under the graph rules. Consequently, parallel drafts may share a failure cause that also affects repeated checks. Distinct attempted tasks are independent within each cell.

## Call measurements

| Role | Base input tokens | Desired generated tokens at low / medium / high | Extra generated tokens if D=1 |
|---|---:|---:|---:|
| Draft | 2,200 | 900 / 2,200 / 5,000 | 1,400 |
| Review | 3,600 | 500 / 1,400 / 3,600 | 800 |
| Repair | 4,200 | 1,000 / 2,500 / 5,500 | 1,200 |
| Merge | 5,000 | 700 / 1,800 / 4,200 | 1,000 |

Input tokens equal the base plus 1,000D. The desired generated count O is subject to the selected cap H. Billing uses min(O,H), including reasoning tokens once alongside visible answer tokens.
The simulator defines completion by O≤H. A truncated call causes rejection under the graph rules below. This cap-to-failure relation is stipulated for the experiment.
The billing model charges short-context, uncached input tokens at the registry's standard rate. It includes the model-call charges defined here, with zero orchestration charges.

Let (I,U) give the input-token count followed by the billed generated count. Call cost is (p_in I+p_out U)/10^6 USD.

| Model | p_in | p_out |
| --- | ---: | ---: |
| Sol | 2 | 10 |
| Sonnet | 2 | 10 |
| Hydrogen | 1.5 | 7.5 |

The stipulated call duration is (1+0.7S)(2+0.008I+0.012U) seconds. Total cost sums every executed call's charge. Elapsed time accumulates over sequential stages, taking the maximum draft duration within a parallel stage. In this simulator, common settings under the shared latent state give the two drafts equal durations.
The simulator holds execution laws fixed across routing mixtures, so the coefficients remain valid as routing probabilities change.

## Outcome generation

For effort e=low/medium/high, define f_e=(1,0.62,0.35). The corresponding detection bases are d_e=(0.65,0.82,0.94).

| Probability | B=1 | B=0 |
| --- | --- | --- |
| Draft defect, p | 1 | (0.12+0.26D+0.06k)f_e |
| Flag given a defective output, d | 0.08 | d_e−0.08D |

A check flags a correct output with probability 0.03.
The simulator retains the result's latent defect status independently of the check verdict. An accepted output with an undetected defect therefore contributes to Y.

The single graph accepts its draft upon completion.

The serial graph reviews its completed draft. A flag triggers a single repair followed by a second review under the original review settings. Every truncated call causes rejection.
Repair defect probabilities are `(1, 0.65p)` in state order `(B=1, B=0)`. The final review uses the original check law. Its clear verdict accepts the repaired draft, with a flag causing rejection.
This is a finite acyclic graph with at most four calls. A clear initial review accepts the original draft directly.

The parallel graph merges two concurrent drafts. A truncated call causes rejection. Conditional on the common state, draft defects are independent Bernoulli(p).
The merge defect probability is one when B=1. When B=0, it depends on how many drafts are defective.

| Defective drafts | Merge defect probability when B=0 |
| --- | --- |
| Both | (0.85,0.70,0.55)_e |
| One | (0.25,0.14,0.07)_e+0.04D |
| Zero | 0.01+0.01D |

The merge requires its result to pass a check within the same call. The model bills the combined operation as one call. Every executed draft incurs its cost, including when the workflow rejects the result.

## Exact population calculation

Enumeration covers each shared state (D,B,S), following every possible subsequent outcome through to the final review decision. The resulting finite terminal distribution has rows of the form (probability, A, Y, C, T, L).
Probability-weighted sums give the population coefficients (a,b,c,ℓ)=(E[A],E[Y],E[C],E[L]) for every candidate within each task group in every scenario. These coefficients are exact up to floating-point arithmetic.
They determine the objective frontier through the population-optimal routing mix at each threshold. Population evaluation of each calibrated rule also uses these coefficients.
`finite-outcomes.csv` retains identical values reached through distinct paths as separate terminal rows. Their probability-weighted summaries appear in `population-moments.csv`.

The linear programme (LP) uses 2×55 routing variables, with z bounding expected cost in every scenario. Two equalities make routing probabilities sum to one within each task group.
Each scenario contributes four inequalities.

| Requirement | Inequality |
| --- | --- |
| Cost upper bound | c−z≤0 |
| Defect limit | b−0.10a≤0 |
| Acceptance floor | −a≤−0.80 |
| Lateness ceiling | ℓ≤0.15 |

SciPy's HiGHS solver computes the solution. The code requires the maximum inequality residual to be below 10^−8.
Every outcome distribution must be normalised subject to Y≤A. Provider checks verify the shared outcome laws, including Hydrogen's cost ratio of 0.75 relative to the remaining providers.

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

Cmax is the maximum possible cost in that cell's finite execution distribution. A deployment needs enforceable limits on the charges possible during execution.

The primary run uses seed 20261114 for n=20,000 per cell. The plotted sample-size illustration uses n in {1,000,3,000,10,000,30,000,100,000}, with seed 20261113+n for each n.
Each size has one separately seeded realisation. The 95% simultaneous coverage guarantee applies to the fixed family of bounds within each experiment. A joint guarantee across sample sizes would require an error allocation covering all five experiments.
Calibration starts with a fixed candidate domain under prespecified sample sizes. After computing the bounds, the LP selects a routing rule. Enumeration supplies that rule's population outcomes, including defect risk among accepted outputs.

## Known-coefficient column search

The restricted set starts with 18 single-call candidates, supplemented by immediate rejection. A Hydrogen serial-review candidate at high effort makes this pool feasible under an 8,192-token cap, giving 20 candidates.
The LP dual solution determines the reduced cost of every omitted candidate for each group using its known population coefficients. The candidate with the most negative reduced cost is added for both groups.
Search stops when all omitted reduced costs exceed −10^−10. After adding three candidates, the fourth solve matches the full-pool optimum within 10^−10 USD per attempt. The final pool contains 23 candidates.
Each iteration inspects the entire omitted pool. Establishing evaluation-cost savings would require candidate selection before outcome data are collected. A learned predictor could be assessed in that experiment.

The frontier solves 69 defect thresholds from 0.03 to 0.20 in increments of 0.0025, with the other constraints fixed. It compares the whole finite pool with the single-call pool plus rejection.
Unplotted grid points are infeasible. Locating the feasibility threshold between adjacent points would require a finer search.

## Scope of the results

The simulator derives outcome coefficients from complete workflows under shared conditions. These coefficients account for conditional execution through repair paths, including rejection at a token cap. Duration calculations include parallel work.
Conservative calibration restricts the feasible set, potentially increasing the objective. A small sample can leave the calibrated programme infeasible.
The numerical optimum applies to these 55 candidates under the stipulated laws. Deployment conclusions require measured workflow outcomes under the intended load. A proposed search policy needs assessment on held-out workloads under an explicit evaluation budget, including when it uses provider advice through a predictor.
