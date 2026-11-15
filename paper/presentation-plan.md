# Selecting LLM graphs and reasoning settings

Lead approach — Patrick Winston. The talk works through one engineering mechanism so that the audience can reproduce the routing decision and identify the evidence it requires.

The result slides borrow Hans Rosling’s practice of explaining the axes before interpreting the pattern. Each numerical claim stays attached to its synthetic experiment.

The intended audience is experienced engineers. The nine-slide arc takes about ten to twelve minutes, including pauses to read the graph and three plots. About 1,200 spoken words allow the presenter to explain the equation at a measured pace. The central idea is that complete executable configurations supply the measured coefficients of a constrained allocation problem.

Use the supplied Global Streaming mark and the six-colour stripe. White surfaces and dark text carry the explanation. The title uses Georgia, with Times New Roman as fallback. Body text uses Arial. The 16:9 slide geometry and readable presentation sizes adapt the supplied report stylesheet. Tables describe exact published mappings. Graphs and charts remain editable where the presentation runtime permits it.

The simulation uses standard uncached text-token tariffs verified on 13 November 2026. It makes no LLM API calls. Its outcome law is shared across providers, so a provider preference in its output reflects the stipulated price comparison. Keep the synthetic response law and tariff scope visible on the relevant slides.



## Slide 1

**Headline**

Selecting LLM graphs and settings

**Visual layout**

Use the actual 16:9 white title layout. The supplied Global Streaming lock-up sits at top right. Two large Georgia lines at left read “Selecting LLM” and “graphs and settings”. A short blue underline separates the title from “Choose an executable workflow mix / that meets measurable service limits.” The footer names Emmett Miller and labels the evidence “Mathematical model + reproducible simulation”. Preserve the six-colour top stripe.

**Spoken script**

This talk shows how to choose an executable LLM workflow and the settings used by each model call. We will use release-note drafting as the running example. A workflow can publish after a check, ask a reviewer for a correction, or stop without publishing. Those choices determine which outputs reach the user.

The aim is to minimise cost while meeting a defect limit and a completion requirement. A deadline adds a further constraint. We will describe each candidate precisely, measure its complete behaviour, and solve for the share of tasks it should receive.

The numerical results come from a reproducible simulation. Published documentation supplies the model interfaces and token prices. The response behaviour is explicitly synthetic. That distinction sets the scope of every result that follows.

[Sources and scope — Paper §§1–2. Graph representation precedent: gptswarm, https://proceedings.mlr.press/v235/zhuge24a.html. Workflow search precedent: aflow, https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html.]


## Slide 2

**Headline**

Choose the graph and every call’s settings

**Visual layout**

Use one editable graph across the left two-thirds. Draft leads to review. Source evidence enters the reviewer from above. A passing review reaches acceptance. A failed review enters a repair-and-recheck node, with passing and failing branches to acceptance and rejection. The upper label states that one repair is allowed. A vertical rule separates the right-hand call settings. Show the exact model ID, followed by its supported effort, compatible controls and role with prompt. The footer says that different calls may use different settings and that edges define which evidence each call receives.

**Spoken script**

Start with the graph. Each model node specifies a task, a model version and a supported reasoning setting. Its other request settings must be compatible with that choice. For example, an output cap may change how much reasoning and answer text the call can produce.

The edges specify the information passed between operations. Giving the reviewer the source change description creates a different candidate from giving it only the draft. The repair condition changes the candidate again. Here the workflow permits one repair, then accepts or rejects the result.

Tools and deterministic checks can be nodes as well. The complete configuration includes timeout behaviour and scheduling. We evaluate that entire execution so that shared mistakes between a drafter and its reviewer remain present in the measured outcomes.

[Sources and scope — Paper §2, equations for θ and node settings. gptswarm, https://proceedings.mlr.press/v235/zhuge24a.html. aflow, https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html.]


## Slide 3

**Headline**

Use published settings to define valid choices

**Visual layout**

Use aligned editable text columns for model ID, selected effort and standard uncached text-token prices per million tokens. The three rows are gpt-6.1-sol with $2.00 input and $10.00 output, claude-sonnet-5-5 at the same tariffs, and gemini-4-hydrogen at $1.50 and $7.50. Each row uses low / medium / high effort. Mark the documentation date as 13 November 2026. A pale blue panel states that response quality, token usage and elapsed time are simulated, with a shared response law across providers. The footer gives the 4,096- and 8,192-token output caps.

**Spoken script**

The registry contains the exact model identifier and its endpoint. It then lists the reasoning controls that the endpoint accepts. Supported controls can differ between models, so the registry also records which other settings are compatible.

For this experiment we selected three documented models. Each supports the low, medium and high settings used here. We combined them with two output caps and three workflow families, then added immediate rejection. This gives fifty-five candidates.

The prices shown are published uncached text-token tariffs. Task correctness, token usage and elapsed time come from stipulated response laws. We use the same response law for each provider. This makes provider selection depend on the tariffs by construction. Provider advice can guide which candidates to evaluate first. Task measurements must determine their outcome probabilities.

[Sources and scope — Registry snapshot 2026-11-13. openaiSol2026, https://developers.openai.com/api/docs/models/gpt-6.1-sol. anthropicSonnet2026, https://platform.claude.com/docs/en/models/sonnet-5-5/overview. gemini_thinking, https://ai.google.dev/gemini-api/docs/thinking. googlePricing2026, https://ai.google.dev/gemini-api/docs/pricing. Full parameter compatibility and billing scope in reproducibility/model-registry.json.]


## Slide 4

**Headline**

A fixed defect limit makes selection linear

**Visual layout**

Define a as the chance of accepting an attempt and b as the chance of accepting an output with a material defect. Place “b / a ≤ ε ⇔ b − εa ≤ 0” in a large pale blue equation field. Beneath it state the worst-scenario expected-cost objective. The lower row shows acceptance at least 80%, accepted defects at most 10% and late attempts at most 15%. The footer identifies these as simulation limits, states the positive-acceptance condition and gives the 180-second deadline.

**Spoken script**

For each complete workflow, let a be the chance that an attempt is accepted. Let b be the chance that it is accepted with a material defect. The defect fraction among accepted outputs is b divided by a.

Fix the allowed fraction epsilon. A positive completion floor keeps the denominator above zero, so the ratio constraint becomes the linear inequality shown here. Acceptance and the joint defect probability both combine linearly when we randomise over complete workflows.

The solver chooses the routing probabilities for each observed task group. It minimises the largest expected cost across the specified scenarios. In this simulation, each scenario requires eighty per cent completion. At most ten per cent of accepted outputs may be defective, and at most fifteen per cent of all attempts may exceed three minutes.

[Sources and scope — Paper §3, finite-configuration theorem and mixture lemma. boyd, https://web.stanford.edu/~boyd/cvxbook/. Simulation limits in reproducibility/simulation-results.json. Cost and lateness count all attempted tasks. Acceptance may include late outputs under the experiment’s declared outcome definition.]


## Slide 5

**Headline**

Search uses the constraints that limit the mix

**Visual layout**

Use a two-row editable process diagram. The top row runs from “Propose graph + settings” through “Evaluate whole workflow” to “Solve selection LP”. A return arrow above the row carries “Dual values guide the next candidate”. A downward connection from the solver is labelled “Freeze candidate set”. The lower row begins with “Collect a fresh sample” on the right. Follow it with “Select using bounds” and end at “Run and monitor the mix”. The footer explains that predictions rank candidates while representative executions establish outcome estimates.

**Spoken script**

Searching every possible graph would usually cost too much. Begin with a feasible set of candidates and solve the allocation problem. Its dual multipliers show which outcome changes would help under the current constraints.

Those multipliers define a reduced-cost score for another graph. A predictor can estimate this score and help choose the next candidate to execute. The complete execution then supplies new evidence. Repeating this process can improve the retained candidate set. A global optimality claim needs a bound covering the entire admissible search space.

Freeze the candidate set after development. Collect a fresh sample and use it to bound the outcome means simultaneously. Those bounds determine which routing weights pass the conservative service checks. For each task, the platform draws a workflow and executes it through its terminal decision. It measures the outcome. New evidence can start another development cycle.

[Sources and scope — Paper §§4–6. dantzigwolfe, https://pubsonline.informs.org/doi/abs/10.1287/opre.8.1.101. smac3, https://www.jmlr.org/papers/v23/21-0888.html. ltt, https://arxiv.org/html/2110.01052v5. The simulation’s exact pricing scans known coefficients and makes no predictor-speed claim.]


## Slide 6

**Headline**

The simulated graph mix meets the 10% limit

**Visual layout**

Use a native editable scatter chart with lines across the left two-thirds. The horizontal axis is accepted-defect limit, in per cent. The vertical axis is US dollars per 1,000 attempts. The full-pool curve is blue and the single-call-only curve is grey. Omit infeasible points and highlight the exact full-pool point at a 10% limit and $38.80 per 1,000 attempts. A large right-side callout repeats that cost and states that single-call mixes are infeasible at this limit. Keep the 55-candidate scope and the two task groups with two scenarios visible. The footer states completion of at least 80% and lateness of at most 15% in each scenario.

**Spoken script**

The horizontal axis varies the allowed defect fraction. The vertical axis shows the lowest worst-scenario cost per thousand attempts that satisfies the remaining requirements. Each point comes from a separate linear programme.

The blue curve permits every candidate family. The grey curve permits single calls and rejection. At a ten per cent defect limit, the full pool is feasible at about 38 dollars and 80 cents per thousand attempts. The single-call pool is infeasible at the same completion and deadline requirements. Adding review workflows changes which trade-offs are available.

This result belongs to the stipulated response law and the fifty-five candidates. The calculation establishes the finite-pool optimum using known synthetic moments. Production quality for each provider requires measured model executions. The next chart shows the routing mixture that produces the highlighted point.

[Sources and scope — Simulation seed 20261113. reproducibility/frontier.csv and simulation-results.json. Exact synthetic moments over the finite experiment. The single-only pool includes rejection. No LLM API calls were made. Provider interfaces and tariffs are documented in model-registry.json.]


## Slide 7

**Headline**

The selected mix changes with the task group

**Visual layout**

Use two native vertical bar charts with the same zero-to-100% scale. The left chart shows routine-task shares of 57.5% for serial low and 42.5% for serial medium. The right chart shows migration shares of 63.6% for serial medium, 30.4% for rejection and 6.1% for single high. Print percentages above the bars. Use matching colours for the same configuration label and grey for rejection. The footer gives the serial output cap of 4,096 tokens and the single-call cap of 8,192. It states that completion targets apply to each scenario as a whole.

**Spoken script**

The router sees whether a task belongs to the routine group or the migration group. It then draws a complete configuration using the probabilities shown here.

Routine tasks use serial review throughout, split between low and medium effort. Migration tasks mostly use serial review at medium effort. About six per cent use a single high-effort call, while about thirty per cent are rejected immediately.

The completion floor applies to the whole scenario. A requirement for every task group would need additional constraints and could change this mixture. All selected model calls use Gemini in this experiment because its lower tariff reduces cost under the shared response law. This is an effect of the simulation design. A real deployment needs measured quality and latency for each eligible provider before its routing shares can be interpreted.

[Sources and scope — reproducibility/routing-plot-data.csv and simulation-results.json, exact known-coefficient solution at ε=.10. Model identifier gemini-4-hydrogen. googlePricing2026, https://ai.google.dev/gemini-api/docs/pricing. Task-group proportions are scenario-dependent, while the conditional routing weights are common across scenarios.]


## Slide 8

**Headline**

Sample size can change which mixes pass the checks

**Visual layout**

Use a native line chart across the left two-thirds with categorical sample sizes 10k, 30k and 100k. The horizontal axis gives trials per configuration × group × scenario. The vertical axis gives US dollars per 1,000 attempts. Show the conservative cost bound in purple with circular markers and the exact synthetic cost of the selected mixture in blue with square markers. The right-side callout states that the bounds permit no feasible mix at 1k and 3k. A second callout gives the main experiment’s 20k trials per cell and 4.32 million trials in total. The footer states the 95% simultaneous coverage construction for each fixed-size synthetic experiment. No risk panel appears on this slide.

**Spoken script**

The previous optimum used exact synthetic moments. For deployment we have to estimate those means. The simulation draws independent trials after freezing the candidate set, then uses those trials to compute simultaneous confidence bounds.

The horizontal axis shows trials per candidate, task group and scenario. The vertical axis gives dollars per thousand attempts. At one thousand and three thousand trials, the conservative programme is infeasible in these draws. That result leaves the true feasible set unresolved. Larger samples tighten the bounds and allow a mixture to be selected.

In the separate twenty-thousand-trial run, the cost bound is about 46 dollars and 83 cents per thousand attempts. The selected mix has a synthetic defect fraction of 8.24 per cent in the adverse scenario. Under the stated sampling and deployment assumptions, the coverage guarantee applies across repeated draws of fresh samples. This single draw illustrates the calculation. Estimating the coverage frequency would require repeated draws.

[Sources and scope — Paper §4. reproducibility/calibration-size.csv and simulation-results.json. Hoeffding radius sqrt(log(4N/α)/(2n)), N=216 non-deterministic cells and α=.05. hoeffding, https://www.cs.rpi.edu/academics/courses/spring06/random/hoefding.pdf. ltt, https://arxiv.org/html/2110.01052v5. Sample-size sensitivity uses separate seeded draws rather than a repeated-run coverage experiment.]


## Slide 9

**Headline**

Test the selected mix under its intended load

**Visual layout**

Use three equally spaced stages with numbered labels and short rules. “Define the service” covers the fixed defect criterion and the completion and deadline requirements. “Measure complete runs” names the current pipeline and a strong single-model baseline as comparisons. “Deploy the fixed rule” asks for queues and costs to be included and for fresh outcomes to be checked against service limits. The closing blue panel says the LP is optimal for its candidate set and valid outcome coefficients. The footer distinguishes the method check in simulation from production evidence of model quality and service performance.

**Spoken script**

To apply this model, begin with a service whose material defects can be assessed independently of the workflow’s own checks. Define completion in terms useful to that service, then state the deadline and task groups available before routing.

Build a versioned registry and execute a manageable set of complete workflows on representative tasks. Compare them with the current pipeline and a strong single-model baseline. Fit outcome predictors on development data and test whether they generalise to withheld graphs. Freeze the retained candidates before collecting the fresh sample used to compute the bounds. Pilot the resulting fixed routing rule under the intended load.

Shared queues can change latency when routing shares change. Monitor that dependence and re-estimate the coefficients when deployment load or task characteristics move beyond the evaluated range. The linear programme solves the finite selection problem using its supplied coefficients. Using the result in production requires representative measurements at the intended load.

[Sources and scope — Paper §§2, 4, 6 and Deployment and scope. The confidence guarantee assumes that conditional moments are invariant to the routing mix and that the fresh sample represents the intended use. The material-defect definition is fixed. openaiDeploy2026, https://developers.openai.com/api/docs/guides/deployment-checklist. Supporting paper and full reproducibility package accompany the presentation.]
