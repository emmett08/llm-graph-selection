# Simulation specification

The finite experiment calculates the cost of meeting service limits under specified execution rules. It uses request settings from provider documentation and bills model calls at published token prices.
The simulator applies the same execution rules to every provider, so the choice of provider depends on token prices. Comparing provider performance would require observations from complete workflow executions.

## Candidate domain

The pool combines three models with three graph families. Sweeping three effort settings at each of two output caps gives 54 configured workflows. Immediate rejection is the 55th candidate.

| Configuration dimension | Values |
| --- | --- |
| Model | `gpt-6.1-sol`, `claude-sonnet-5-5`, `gemini-4-hydrogen` |
| Graph | Single call, serial review with repair, parallel drafts with a merge |
| Effort | `low`, `medium`, `high` |
| Generated-token cap per call | {4,096, 8,192} |

Every call within a configured graph uses the same model and request settings. The general model supports settings that vary by node. A broader experiment could vary those settings and test changes to prompts or to which calls see the source evidence.

The router observes task group g under the mapping `{0: routine, 1: migration}`. The simulator makes the observed label equal to the true task group and assigns zero cost to routing and orchestration. In a deployment, the classifier can supply a different observed group. Evaluating service within true groups then requires independently labelled tasks and outcome estimates for the joint combinations of observed and true group.
Scenario k defines operating conditions for a specified task distribution.

| Scenario k | Conditions | Routine proportion | Migration proportion |
| --- | --- | ---: | ---: |
| 0 | Nominal | 0.70 | 0.30 |
| 1 | Adverse | 0.40 | 0.60 |

Routing depends on the observed group alone, so the same group-specific probabilities apply in every scenario.

The simulator records the following outcomes for each attempt.

| Symbol | Outcome |
| --- | --- |
| A | Indicator that the workflow releases a result under the terminal service rule |
| Y | Acceptance with a material defect, so Y≤A |
| C | Total charge in USD |
| T | Elapsed time in seconds |
| L | Lateness indicator, L=1{T>180} |

The baseline acceptance rule permits late outputs, which have A=L=1. Every rejected attempt retains its measurements (C,T). Immediate rejection has A=Y=C=T=L=0.
Each scenario requires a minimum acceptance rate of 0.80. Up to 0.10 of accepted outputs may have a material defect, and at most 0.15 of all attempts may exceed the deadline. The objective minimises the highest expected cost per original attempt across scenarios. Acceptance, lateness and mean cost use every original attempt in their denominators.

When acceptance has to occur by the deadline, use A′=A(1−L) and Y′=Y(1−L). This changes which outcomes count towards the minimum acceptance rate and which accepted outputs enter the defect calculation. The joint on-time and defect-free rate averages (A−Y)(1−L) over all attempts. It measures whether the attempt delivered an accepted result by the deadline without a material defect.

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

Input tokens equal the base plus 1,000D. Let O be the desired generated-token count and H the selected cap. The billed number is min(O,H), with each reasoning or visible answer token counted once.
The simulator completes a call when O≤H. If O>H, the call is truncated and the workflow rejects the attempt.
The billing model charges short-context, uncached input tokens at the registry's standard rate. It includes the model-call charges defined here, with zero orchestration charges.

Let (I,U) give the input-token count followed by the billed generated count. Call cost is (p_in I+p_out U)/10^6 USD.

| Model | p_in | p_out |
| --- | ---: | ---: |
| Sol | 2 | 10 |
| Sonnet | 2 | 10 |
| Hydrogen | 1.5 | 7.5 |

The simulator calculates call duration in seconds as (1+0.7S)(2+0.008I+0.012U). Total cost sums every executed call's charge. Elapsed time accumulates over sequential stages, taking the maximum draft duration within a parallel stage. In this simulator, common settings under the shared latent state give the two drafts equal durations.
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

For each shared state (D,B,S), enumerate every possible call outcome through to the workflow's terminal decision. The resulting finite terminal distribution has rows of the form (probability, A, Y, C, T, L).
Probability-weighted sums give the population coefficients (a,b,c,ℓ)=(E[A],E[Y],E[C],E[L]) for every candidate within each task group in every scenario. These coefficients are exact up to floating-point arithmetic.
At each threshold, solve the optimisation with these coefficients to obtain the population frontier. Use them again to evaluate the population outcomes of rules selected using sample bounds.
`finite-outcomes.csv` retains identical values reached through distinct paths as separate terminal rows. Their probability-weighted summaries appear in `population-moments.csv`.

The linear programme (LP) uses 2×55 routing variables, with z bounding expected cost in every scenario. Two equalities make routing probabilities sum to one within each task group.
Each scenario contributes four inequalities.

| Requirement | Inequality |
| --- | --- |
| Cost upper bound | c−z≤0 |
| Defect limit | b−0.10a≤0 |
| Minimum acceptance rate | −a≤−0.80 |
| Lateness ceiling | ℓ≤0.15 |

SciPy's HiGHS solver computes the solution. The code requires the maximum inequality residual to be below 10^−8.
Every outcome distribution must be normalised subject to Y≤A. Provider checks verify the shared outcome laws, including Hydrogen's cost ratio of 0.75 relative to the remaining providers.

## Confidence bounds for the frozen pool

The sampling procedure treats the coefficients of N=54×2×2=216 cells as unknown. The immediate-rejection coefficients are known.
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
Fix the candidate pool and sample sizes before drawing the samples. The LP then selects a routing rule using the confidence bounds. Evaluate that rule against the enumerated population coefficients to recover its service outcomes, including the defect fraction among accepted outputs.

## Known-coefficient column search

The restricted set starts with 18 single-call candidates, supplemented by immediate rejection. A Hydrogen serial-review candidate at high effort makes this pool feasible under an 8,192-token cap, giving 20 candidates.
The LP dual solution determines the reduced cost of every omitted candidate for each group using its known population coefficients. The candidate with the most negative reduced cost is added for both groups.
Search stops when all omitted reduced costs exceed −10^−10. After adding three candidates, the fourth solve matches the full-pool optimum within 10^−10 USD per attempt. The final pool contains 23 candidates.
Each iteration inspects the entire omitted pool. Establishing evaluation-cost savings would require candidate selection before outcome data are collected. A learned predictor could be assessed in that experiment.

The frontier solves 69 defect thresholds from 0.03 to 0.20 in increments of 0.0025, with the other constraints fixed. It compares the whole finite pool with the single-call pool plus rejection.
Unplotted grid points are infeasible. Locating the feasibility threshold between adjacent points would require a finer search.

## Sensitivity analyses

`refine_selection.py` reruns the population calculations with changes to defect probabilities, the candidate pool, scenario weights or service requirements.

| Change | Inputs used in the comparison |
| --- | --- |
| Lower single-call defect rates | Multiply the joint defective-acceptance coefficient by 0.75 in every single-call cell. Keep acceptance, cost, lateness and all multicall coefficients fixed. |
| Change the serial repair outcome | Outside a shared blind spot, make the probability of a defect after repair depend on whether the first review detected a defect or falsely flagged a sound draft. Apply the final review under the original rules. |
| Remove immediate rejection | Exclude the zero-outcome candidate. Other workflows retain their original acceptance and rejection rules. |
| Shift the adverse task mix | Vary the adverse migration share between 0.59 and 0.61 with complementary routine shares and fixed per-group outcome coefficients. Keep the nominal scenario as specified above. |
| Change the service scope | Apply the service limits within each represented task group, require acceptance by the deadline, or combine those requirements. Calculate on-time outcomes from the joint terminal distributions. |

For completed repairs in the serial comparison, let r=0.65p, the baseline repair defect probability when B=0. If the initial draft has a detected defect, the repair retains it with probability 0.50. If the repair removes it, a new defect occurs with probability r. The revised draft therefore has a material defect with probability 0.50+0.50r. If the review falsely flags a sound draft, the repair introduces a defect with probability r and adds a 0.20 probability of introducing a defect conditional on the baseline repair producing a sound result. The resulting probability is r+(1−r)×0.20. A shared blind spot still makes the revised draft defective. The first and final review probabilities, call costs and durations follow the original rules. These probabilities are inputs to a synthetic comparison.

## Scope of the results

The simulator derives outcome coefficients from complete workflows under shared conditions. These coefficients account for conditional execution through repair paths, including rejection at a token cap. Duration calculations include parallel work.
Using conservative confidence bounds restricts the feasible set and can increase the minimum cost. A small sample can produce bounds too wide for any routing rule to meet all the constraints.
The numerical optimum applies to these 55 candidates with the simulator's defect probabilities, token counts and call durations. Deployment conclusions require measured workflow outcomes under the intended load. A proposed search policy needs assessment on held-out workloads under an explicit evaluation budget, including when it uses provider advice through a predictor.
