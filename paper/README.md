# Choosing LLM workflow topologies under cost and failure constraints

This package contains the manuscript revision of 17 November 2026 and the presentation plan for the earlier conditional-defect study.
The manuscript is an unreviewed draft in the American Mathematical Society's article layout.

[Read the compiled paper](llm-graph-selection-paper.pdf).

## Read the model

`main.tex` formulates topology selection as a cost–failure decision and derives a linear programme for routing mixtures under fixed outcome means. The finite simulation compares one fixed configuration, one topology family with varying settings and routing across families. The paper proposes a Kubernetes alert-triage study to measure operational outcomes and test how telemetry access affects the propagation of an upstream error.
`references.bib` contains the references. See `reproducibility/verified-sources.md` for the evidence locations.
`presentation-plan.md` specifies the presentation slide by slide, including the spoken script. The editable presentation accompanies this package separately.

The simulation combines three models with three graph families. Sweeping three effort levels at each of two output caps gives 54 configured workflows. Immediate rejection is the 55th candidate.
Every call within a simulated graph uses the same model and request settings. The general formulation lets each node have its own settings.
Provider documentation supplies valid request settings and token prices. The simulator specifies how the calls behave and how it calculates their token use and duration. It applies those rules equally to every provider, so provider choice depends on token prices.

The main comparison retains the original acceptance, conditional defect and lateness limits and adds a ceiling on the share of all attempts that fail. For the simulation, failure means no timely, defect-free accepted result. Each original task stays in the denominator. A deployment defines success from the service obligation, including any acceptable handoff or required fallback. The simulator supplies the true group to its router. A live evaluation routes on the observed group and assesses service needs using independently assigned labels.

## Reproduce the results

From the `paper/` directory, use Python 3.12 with the pinned dependencies.

```sh
python -m pip install -r reproducibility/requirements.txt
python reproducibility/simulate_workflows.py
python reproducibility/verify_exact_example.py
python reproducibility/refine_selection.py
python reproducibility/fractional_check.py
python reproducibility/joint_failure_frontier.py --check
```

After dependency installation, the scripts run locally from the supplied simulation laws. The simulator overwrites its generated outputs, including the three data figures. The sensitivity analyses and exact checks have separate outputs. Run `python reproducibility/joint_failure_frontier.py` to regenerate the all-task frontier CSV and its TikZ figure. Use `--failure-cap .30` to inspect the comparison at a 30% ceiling.
`reproducibility/README.md` explains each file's role in the experiment. The complete conditional rules appear in `reproducibility/simulation-method.md`, including every constant.

## Build the paper

Use a TeX distribution such as TeX Live, including `latexmk` with the packages imported by `main.tex`. MacTeX is suitable on macOS. Run the build from this directory.

```sh
bash check-paper.sh
```

The compiled paper is `build/main.pdf`. The build check validates citations and references. It fails on LaTeX errors or overfull boxes and checks the source for placeholder text.
After checking the rendered pages, copy `build/main.pdf` to `llm-graph-selection-paper.pdf` to update the version linked above.

The first two figures use the TikZ `graphs` library. Their editable sources are included at full size from the following files, each with a corresponding `.caption.tex` file.

| Figure | Source |
| --- | --- |
| Workflow execution | `figures/workflow-execution.tikz.tex` |
| Workflow topologies | `figures/workflow-topologies.tikz.tex` |
| Cost and all-task failure | `figures/joint-failure-frontier.tikz.tex` |

The cost–failure frontier renders into the manuscript as a vector figure. The earlier figures have standalone vector PDFs and the earlier data figures have PNG copies.
The figure specifications explain how the inputs determine the visual marks. Review records retain the layout checks. Project paths are relative to `paper/`, with environment paths identifying the original rendering setup.

## Validation

An independent review checked the proof for finite mixtures and the confidence guarantee for the selected rule. It also checked the reduced-cost argument and its approximate optimality gap bound.
Rational arithmetic verifies the small appendix example by matching the cost of a feasible rule to a dual lower bound.
The simulation validates each outcome distribution before checking the selected rules against the service constraints. With known coefficients, the reduced-cost search must reproduce the full-pool optimum.

The original four-mean calibration comparison uses outcome counts equivalent to 4.32 million synthetic trials across 216 cells whose coefficients are treated as unknown.
Sensitivity analyses reduce the probability that a single call yields an accepted result with a material defect and test the effect of excluding immediate rejection. The serial review sensitivity lets repair retain an existing defect or introduce one after a false flag. Further analyses vary the adverse higher-difficulty share between 59% and 61%, apply service limits within each group, or require acceptance by the deadline. The timing analyses also test changes in queue delay. Each estimate describes the simulator under the inputs used for that comparison.

All cited sources have a retained verification entry. The nine slides from 13 November 2026 were visually reviewed.

The numerical results use the simulator's assigned outcome probabilities. The proposed shadow study would compare the incumbent triage process, a single tool-using agent and the candidate multi-agent workflows on the same eligible alerts. When shared capacity makes outcomes depend on routing weights, run independent workload repetitions for each deployment design at the intended load.
