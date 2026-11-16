# Selecting LLM workflow configurations

This package contains the manuscript revision of 16 November 2026, accompanied by the presentation plan from 13 November 2026.
The manuscript is an unreviewed draft in the American Mathematical Society's article layout.

[Read the compiled paper](llm-graph-selection-paper.pdf).

## Read the model

`main.tex` proves the model's results before applying them to a finite simulation. The workflow graph gives the dependencies that determine which calls may run next. The paper proposes a release-note study to test how a reviewer's access to the change description affects the cost of meeting service limits.
`references.bib` contains the references. See `reproducibility/verified-sources.md` for the evidence locations.
`presentation-plan.md` specifies the presentation slide by slide, including the spoken script. The editable presentation accompanies this package separately.

The simulation combines three models with three graph families. Sweeping three effort levels at each of two output caps gives 54 configured workflows. Immediate rejection is the 55th candidate.
Every call within a simulated graph uses the same model and request settings. The general formulation lets each node have its own settings.
Provider documentation supplies valid request settings and token prices. The simulator specifies how the calls behave and how it calculates their token use and duration. It applies those rules equally to every provider, so provider choice depends on token prices.

The main example minimises cost per original attempt under service limits that apply across each scenario. A result counts as accepted when the workflow releases it under the service's terminal rule. For release notes, the corresponding requirement is a minimum publication rate. Rejected attempts stay in the denominator, and late outputs can still count as accepted. An on-time accepted result and an on-time accepted result free of material defects are separate outcomes. In the simulator, the router receives the true task group. An operational evaluation can route on the classifier's observed group and assess service outcomes using independently labelled true groups.

## Reproduce the results

From the `paper/` directory, use Python 3.12 with the pinned dependencies.

```sh
python -m pip install -r reproducibility/requirements.txt
python reproducibility/simulate_workflows.py
python reproducibility/verify_exact_example.py
python reproducibility/refine_selection.py
python reproducibility/fractional_check.py
```

After dependency installation, the scripts run locally from the supplied simulation laws. The simulator overwrites its generated outputs, including the three data figures. The sensitivity analyses and exact checks have separate outputs.
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

Every figure has a standalone vector PDF. Data figures also have PNG copies for reuse.
The figure specifications explain how the inputs determine the visual marks. Review records retain the layout checks. Project paths are relative to `paper/`, with environment paths identifying the original rendering setup.

## Validation

An independent review checked the proof for finite mixtures and the confidence guarantee for the selected rule. It also checked the reduced-cost argument and its approximate optimality gap bound.
Rational arithmetic verifies the small appendix example by matching the cost of a feasible rule to a dual lower bound.
The simulation validates each outcome distribution before checking the selected rules against the service constraints. With known coefficients, the reduced-cost search must reproduce the full-pool optimum.

The main confidence bounds use outcome counts equivalent to 4.32 million synthetic trials across 216 cells whose coefficients are treated as unknown.
Sensitivity analyses reduce the probability that a single call yields an accepted result with a material defect and test the effect of excluding immediate rejection. The serial review sensitivity lets repair retain an existing defect or introduce one after a false flag. Further analyses vary the adverse migration share between 59% and 61%, apply service limits within each group, or require acceptance by the deadline. The timing analyses also test changes in queue delay. Each estimate describes the simulator under the inputs used for that comparison.

All cited sources have a retained verification entry. The nine slides from 13 November 2026 were visually reviewed.

The results describe the workflows in the finite simulator. A production pilot would compare complete workflows on representative tasks under the intended load, using the same service limits for the selected mix, the current pipeline and a tuned single-call baseline. Where the routing mix affects outcomes through shared capacity, evaluate complete policies at the intended load.
