# Source verification ledger

Verified on 13 November 2026. This ledger records the primary sources used for the model, references and provider registry. The dates on living documentation entries are retrieval years, not claims about first publication. Titles, identities, effort controls and listed prices were checked against provider pages. No live API evaluation was performed.

## Research and mathematical sources

| BibTeX key | Verified metadata and source | Source location supporting use in the paper | Scope of support |
|---|---|---|---|
| `gptswarm` | Zhuge et al. (2024). *GPTSwarm: Language Agents as Optimizable Graphs*. ICML, PMLR 235, 62743–62767. [Publisher record and BibTeX](https://proceedings.mlr.press/v235/zhuge24a.html). | Publisher abstract describes operation nodes, information-flow edges, prompt optimisation and connectivity optimisation. | Establishes prior graph abstraction and graph optimisation. It does not establish that this paper's combined allocation procedure is optimal over all possible workflows. |
| `aflow` | Zhang et al. (2025). *AFlow: Automating Agentic Workflow Generation*. ICLR. [Conference record](https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html). [Author manuscript, version 4](https://arxiv.org/html/2410.10762v4). | Section 3.1 defines node model, prompt, temperature and output format. Section 3.2 fixes model, temperature and format in its implemented search. Section 4 explains the MCTS cycle. | Supports graph/code workflow search and the distinction between a broad formulation and a restricted experimental search space. |
| `smac3` | Lindauer et al. (2022). *SMAC3: A Versatile Bayesian Optimization Package for Hyperparameter Optimization*. JMLR 23(54), 1–9. [Publisher record](https://www.jmlr.org/papers/v23/21-0888.html). [Full paper](https://jmlr.org/papers/volume23/21-0888/21-0888.pdf). | Section 2.2, page 3, equation (2). The selected algorithm activates its hyperparameter subspace, with random forests modelling conditional configurations. | Supports Bayesian optimisation as a practical option for conditional model and hyperparameter choice. It supplies no universal prediction ranking for this application. |
| `ltt` | Angelopoulos, Bates, Candès, Jordan and Lei (2025). *Learn then Test: Calibrating Predictive Algorithms to Achieve Risk Control*. AOAS 19(2), 1641–1662. DOI [10.1214/24-AOAS1998](https://doi.org/10.1214/24-AOAS1998). [Author manuscript, version 5](https://arxiv.org/html/2110.01052v5). Journal metadata cross-checked on [Candès's Stanford profile](https://profiles.stanford.edu/emmanuel-candes?tab=bio). | Section 2 describes testing-based risk control. Section 2.5, Proposition 7, states the uniform-bound alternative. | Supports the principle of selecting a configuration with statistically controlled risk. The paper's finite-family Hoeffding construction is stated and proved separately. The author manuscript also warns that uniform concentration can be conservative. |
| `hoeffding` | Hoeffding (1963). *Probability Inequalities for Sums of Bounded Random Variables*. JASA 58(301), 13–30. DOI [10.1080/01621459.1963.10500830](https://doi.org/10.1080/01621459.1963.10500830). [Full original paper hosted by RPI](https://www.cs.rpi.edu/academics/courses/spring06/random/hoefding.pdf). | Section 2, Theorem 1, equation (2.3), printed page 14, gives the exponential bound for independent variables in [0,1]. Theorem 2 extends it to unequal bounded ranges. Scanned page checked visually. | Supports the concentration step. The simultaneous bound additionally uses a union bound and the fixed-family, held-out assumptions stated in this paper. |
| `dantzigwolfe` | Dantzig and Wolfe (1960). *Decomposition Principle for Linear Programs*. Operations Research 8(1), 101–111. DOI [10.1287/opre.8.1.101](https://doi.org/10.1287/opre.8.1.101). [Publisher record](https://pubsonline.informs.org/doi/abs/10.1287/opre.8.1.101). | Abstract describes the coordinating programme, subproblems producing columns and the coordinator's prices. Publisher citation supplies volume, issue and page range. | Supports the column-generation architecture. Exact global optimality requires an exact pricing oracle, which a heuristic workflow search does not supply. |
| `boyd` | Boyd and Vandenberghe (2004). *Convex Optimization*. Cambridge University Press. DOI [10.1017/CBO9780511804441](https://doi.org/10.1017/CBO9780511804441). [Author page](https://web.stanford.edu/~boyd/cvxbook/), [co-author page](https://www.seas.ucla.edu/~vandenbe/cvxbook.html), [publisher metadata](https://www.cambridge.org/highereducation/books/convex-optimization/17D2FAA54F641A2F62C7CCD01DFA97C4). | Chapter 4 is the reference for convex optimisation problems and linear programmes. Author pages confirm title, authors, publisher and 2004 date. | General optimisation background. The construction and exact scope of the particular workflow LP are derived in the paper. |

The publisher's LTT page served an access-control notice. Its author manuscript supplied the mathematical content, and the author's university publication record supplied journal pages and DOI. The publisher's Dantzig–Wolfe abstract and citation were readable. No unavailable full text is represented here as having been inspected.

## Published provider registry

The simulation uses a deliberately small subset of documented models and settings. All three providers support low, medium and high for the selected model. These labels are categorical provider controls. They do not define equal computational budgets, equal capability, or a shared cardinal scale.

| Provider model | Selected control | Published additional settings | Standard uncached text price, USD per million input/output tokens | Principal source |
|---|---|---|---|---|
| `gpt-6.1-sol` | `reasoning.effort` low, medium, high with standard reasoning mode | xhigh, max. Default effort medium. | 2.00 / 10.00 | [Model page](https://developers.openai.com/api/docs/models/gpt-6.1-sol) |
| `claude-sonnet-5-5` | `output_config.effort` low, medium, high with adaptive thinking | xhigh, max. Default effort high. | 2.00 / 10.00 | [Model overview](https://platform.claude.com/docs/en/models/sonnet-5-5/overview) and [effort guide](https://platform.claude.com/docs/en/build-with-claude/effort) |
| `gemini-4-hydrogen` | `generation_config.thinking_level` low, medium, high on Interactions API | xhigh. Default effort high. | 1.50 / 7.50 | [Thinking guide](https://ai.google.dev/gemini-api/docs/thinking) and [pricing table](https://ai.google.dev/gemini-api/docs/pricing) |

The tariff calculation uses the standard direct API tier and short uncached text requests. It excludes batch, priority, regional, tool, cache and tax charges. OpenAI's price page applies higher rates above 272,000 input tokens. The simulated calls remain below that threshold. Reasoning tokens form part of the billable output total and must not be added to that total twice. Dollar results combine published tariffs with synthetic token counts, so they are simulated charges rather than observed invoices.

### OpenAI

`openaiSol2026` is [GPT-6.1 Sol model documentation](https://developers.openai.com/api/docs/models/gpt-6.1-sol). The model description and reasoning-effort line verify the identity and the five values. The pricing table supplies the uncached input and output rates. The page lists a 128,000-token output maximum.

`openai_reasoning` is [Reasoning models](https://developers.openai.com/api/docs/guides/reasoning). The mode and effort discussion distinguishes standard/pro mode from effort within a mode. The registry uses standard.

`openaiDeploy2026` is [API deployment checklist](https://developers.openai.com/api/docs/guides/deployment-checklist), sections on migration and reasoning effort. Active reasoning excludes temperature, top_p and top_logprobs. The provider describes higher effort as allocating more reasoning work for planning, debugging and multi-step tasks. This is design advice, not a measured coefficient in the simulation.

### Anthropic

`anthropicSonnet2026` is [Claude Sonnet 5.5 overview](https://platform.claude.com/docs/en/models/sonnet-5-5/overview). Its identity, pricing, token-limit and migration sections verify the model ID, USD 2/10 rates, 128,000-token output limit and removal of temperature, top_p and top_k.

`anthropic_effort` is [Effort](https://platform.claude.com/docs/en/build-with-claude/effort), the model-specific guidance for Sonnet 5.5. The provider suggests medium for well-specified agentic coding and tool work, higher effort for harder work, and low or medium when latency matters. Adaptive thinking is selected in the registry. The simulation does not assign quality scores from this prose.

### Google

`gemini_thinking` is [Gemini thinking](https://ai.google.dev/gemini-api/docs/thinking), the model support table and the section explaining thinking levels. The table verifies all four Hydrogen levels and the high default. The guide recommends comparing levels on evaluations and choosing the lowest one meeting the task's quality requirement.

`googlePricing2026` is [Gemini Developer API pricing](https://ai.google.dev/gemini-api/docs/pricing), the Gemini 4 Hydrogen Standard table. It lists USD 1.50 input and USD 7.50 output per million tokens, with thinking included in output pricing.

`googleHydrogen2026` is [Developer's guide to Gemini 4 Hydrogen](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/guides/gemini-4-hydrogen). The mandatory API rules section verifies lowercase levels on the Interactions API, the unified input-plus-output budget, the output cap and sampling behaviour. Custom temperature, top_k and top_p are ignored. Thinking_budget, frequency_penalty, presence_penalty and candidate_count above one are invalid. The selected 4,096/8,192 output caps fit below the 65,536-token foreground segment limit, with sufficient remaining context.

Google's developer thinking and pricing pages redirected to `https://aistudio.google.com/docs/thinking` and `https://aistudio.google.com/docs/pricing` during verification. The original official URLs remain the bibliography locators.

## Boundary between evidence and simulation

Published evidence supplies model identifiers, API eligibility rules, effort choices, selected default modes, list prices and qualitative task advice. The workload distribution, token counts, latency laws, task success laws, difficulty effects, correlation, repair effects and any drift scenario are explicitly synthetic. The simulation is a test of the proposed selection mechanism under declared assumptions. It does not rank these providers on measured quality or speed.

The 4,096/8,192 caps are study design choices inside supported limits. Provider advice may guide which trials to run first. It is not treated as a probability of success or as a guarantee. Deployment requires refreshed documentation, endpoint validation and measurements on the target workload.

## Sources added to the mathematical revision

| BibTeX key | Primary evidence checked on 14 November 2026 | Use |
| --- | --- | --- |
| `charnesCooper` | [Wiley article, DOI 10.1002/nav.3800090303](https://onlinelibrary.wiley.com/doi/10.1002/nav.3800090303) and [Boyd and Vandenberghe, §4.3.2](https://www.stanford.edu/~boyd/cvxbook/bv_cvxbook.pdf) | Single-denominator linear-fractional transformation. The textbook supplies the proof and conditions. |
| `maurerPontil` | [Author paper, Theorem 4 and Corollary 5](https://arxiv.org/pdf/0907.3740) | One-sided bound for independent, identically distributed scores in a unit interval. Shifting the paired score to this interval preserves its variance. |
