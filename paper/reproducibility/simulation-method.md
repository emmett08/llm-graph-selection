# Simulation specification

This is a finite synthetic experiment. No LLM was called and no observed provider quality, speed or token usage is reported. Provider documentation supplies the legal model–effort settings and the quoted input/output prices. Every task, failure, usage and duration distribution below is stipulated. The mechanism is exactly the same for every model identity; only token prices differ. Thus any selected provider reflects these stipulated equal outcomes and the quoted prices.

## Candidate domain and evaluation target

The pool has 54 configured workflows and one reject action: three registered models × three graph families × three effort settings × two output caps. The registry fixes `gpt-6.1-sol`, `claude-sonnet-5-5` and `gemini-4-hydrogen`; common effort settings are `low`, `medium` and `high`, and caps are 4,096 and 8,192 generated tokens per call. Every node in a configured graph uses the same model, effort and cap. The experiment therefore exercises a restricted subspace of the mathematical model, which permits differing settings across nodes. It does not vary source-access edges or prompts.

The router observes group g: routine (0) or migration (1). Group labels are supplied without error or classification cost; classifier and orchestration overhead are zero. It does not observe scenario k: nominal (0) or adverse (1). Routine/migration proportions are (0.70, 0.30) in the nominal scenario and (0.40, 0.60) in the adverse scenario. The router uses the same group-specific randomisation in both scenarios.

For each attempt, A indicates acceptance; Y indicates an accepted defective output, so Y≤A; C is the charge in USD; T is seconds; and L=1{T>180}. Acceptance does not require completion before the deadline. Rejected attempts retain their incurred cost and duration. Immediate rejection has A=Y=C=T=L=0. The constraints are acceptance ≥0.80, defects among accepted outputs ≤0.10 and lateness among all attempts ≤0.15, separately in both scenarios. The objective is the maximum scenario expected cost per attempted task.

## Shared latent state

Each attempt draws difficulty D with probability 0.15+0.40g+0.10k, blind spot B conditional on D with probability 0.01+0.02D+0.01k, and slow load S with probability 0.06+0.08k. S is independent of (D,B). The service multiplier is 1+0.7S. D, B and S are shared by all calls in the graph. Subsequent error and review draws are conditionally independent given these states, their parents and the graph rules. This creates dependence between parallel drafts and between repeated checks without assuming independent node-level quality unconditionally. Distinct attempted tasks are independent within each cell.

## Synthetic usage and duration

| Role | Base input tokens | Desired generated tokens at low / medium / high | Extra generated tokens if D=1 |
|---|---:|---:|---:|
| Draft | 2,200 | 900 / 2,200 / 5,000 | 1,400 |
| Review | 3,600 | 500 / 1,400 / 3,600 | 800 |
| Repair | 4,200 | 1,000 / 2,500 / 5,500 | 1,200 |
| Merge | 5,000 | 700 / 1,800 / 4,200 | 1,000 |

Input tokens equal the base plus 1,000D. Let the desired generated count be O and selected cap be H. The billed generated count is min(O,H). These synthetic generated counts include reasoning tokens once; they are not counts of visible answer text. A call completes if and only if O≤H. A truncated call causes rejection according to the graph control rules below. This exact cap-to-failure relation is a simulation assumption, not a claim about any provider's behaviour. All input tokens are short-context, uncached and charged at the registry's standard rate. There are no tools, cache charges, batches, tier differences or orchestration charges.

For input count I and billed generated count U, call cost is (p_in I+p_out U)/10^6 USD. Rates p_in/p_out are 2/10 for Sol and Sonnet and 1.5/7.5 for Hydrogen. Synthetic call duration is (1+0.7S)(2+0.008I+0.012U) seconds. This duration law is not derived from provider measurements. Costs add over executed calls. Durations add along serial calls; parallel draft time is their maximum, which equals either draft duration here because both drafts share their settings and latent state. No queue interaction depends on the routing mix; coefficients therefore remain fixed under mixing.

## Error and check laws

For effort e=low/medium/high define multipliers f_e=(1,0.62,0.35) and detection bases d_e=(0.65,0.82,0.94). The draft defect probability is p=1 when B=1, and p=(0.12+0.26D+0.06k)f_e otherwise. A check flags a defective output with probability d=0.08 when B=1, and d=d_e−0.08D otherwise. It flags a correct output with probability 0.03. Check outcomes do not constitute the independent evaluation label: the simulator separately retains the latent correctness of the result, so an undetected defect contributes to Y.

The single graph calls one draft and accepts if it completes.

The serial graph calls a draft, then review. Truncation of either rejects. If review passes, it accepts the original draft. If review flags, it calls repair and then a second review. Repair is defective with probability one when B=1 and 0.65p otherwise. A truncated repair rejects. A completed repair is accepted only if the second review passes. There is at most one repair, and the second review uses the same role/cap and check law as the first. Thus this branch is a finite acyclic graph with up to four calls.

The parallel graph calls two drafts concurrently, then merge. Either draft or merge truncation rejects. Conditional on the common state, draft defects are independent Bernoulli(p). The merge defect probability is one when B=1. Otherwise it is (0.85,0.70,0.55)_e when both drafts are defective, (0.25,0.14,0.07)_e+0.04D when one is defective, and 0.01+0.01D when neither is defective. Merge applies the same check law to its result within its single call; it accepts only on a pass. This is a stipulated combined merge-and-check node, not an extra uncharged review. All executed draft costs are charged even when the result is rejected.

## Exact population calculation

The script enumerates D, B, S, each draft/repair/merge correctness event and each review decision. It constructs a finite distribution of terminal rows (probability, A, Y, C, T, L). It sums those rows to obtain a=E[A], b=E[Y], c=E[C] and ℓ=E[L] for every group–candidate–scenario cell. These population coefficients are exact up to floating-point arithmetic. No Monte Carlo approximation is used for the objective frontier, exact selected mix, or evaluation of any calibrated rule. `finite-outcomes.csv` records each terminal row, including duplicate-valued rows that originate from distinct paths. `population-moments.csv` records the sums.

The LP uses 2×55 routing variables plus its cost epigraph. It has two simplex equalities and eight scenario inequalities. For each scenario, those inequalities are c−z≤0, b−0.10a≤0, −a≤−0.80 and ℓ≤0.15. SciPy's HiGHS solver is used. The code requires the maximum inequality residual to be below 10^−8. It checks every probability distribution sums to one and that Y≤A, and checks identical outcome coefficients across providers and a 0.75 cost ratio for Hydrogen.

## Frozen-pool calibration

There are N=54×2×2=216 nontrivial cells. The immediate reject action is exactly known and need not be calibrated. For each remaining cell, one multinomial draw of n terminal outcomes is equivalent to n independent attempts from the enumerated population distribution. Trials are independent between cells. A simultaneous one-sided Hoeffding construction at family error α=0.05 uses r=sqrt(log(4N/α)/(2n)): lower acceptance max(0,â−r), upper joint defect min(1,b̂+r), upper lateness min(1,ℓ̂+r) and upper cost min(Cmax,ĉ+Cmax r). Cmax is the maximum possible cost in that cell's known finite execution distribution. This is an actual finite simulator bound; a deployment must establish its own bound from enforceable execution and charging limits.

The primary run uses n=20,000 per cell and seed 20261114. The plotted sample-size illustration uses n in {1,000,3,000,10,000,30,000,100,000}, with seed 20261113+n for each n. Each size is a separate single realisation, not a replication-averaged learning curve. The 95% statement applies to the fixed family in each calibration experiment; the five plotted experiments are not asserted to have joint 95% coverage. Neither sample size nor candidate domain is chosen after inspecting that calibration sample. Once the bounds are computed, the LP selects a rule; its exact population cost/risk are evaluated from the enumerated distribution.

## Known-coefficient column search

The initial restricted set contains all 18 single-call candidates, immediate rejection and a fixed Hydrogen serial/high/8192 feasibility seed: 20 candidates total. The LP duals price every omitted candidate for each group using its already-known population coefficients. The most negative reduced-cost candidate is added for both groups. Search stops when all omitted reduced costs exceed −10^−10. The stopping rule reaches the full-pool optimum after adding three candidates (23 retained, four solves), verified within 10^−10 USD. Pricing inspects the full omitted pool at each iteration. This is a computational demonstration of column generation, not a reduction in the cost of acquiring LLM evaluation data and not a comparison with a learned search predictor.

The frontier solves 69 defect thresholds from 0.03 to 0.20 in increments of 0.0025 with other constraints fixed, for the whole finite pool and for single calls plus rejection. Unplotted values are infeasible; no exact feasibility threshold between grid points is inferred.

## Scope of the results

The simulator demonstrates that whole-graph nonlinear behaviour, shared failure causes, conditional repair, cap failures and parallel time can feed a linear mixture selector. It also demonstrates how conservative calibration can make a finite pool infeasible at small sample sizes and raise the objective when feasible. It does not validate the numerical failure mechanism, identify a superior provider or predictor, prove optimality beyond the 55 candidates, estimate real production latency, or evaluate published advice as a search prior. Predictions and advice-based proposal policies remain proposals requiring held-out workload evaluation.
