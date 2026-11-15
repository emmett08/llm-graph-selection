# Selecting LLM workflow configurations

This package contains the manuscript revision of 15 November 2026, accompanied by the presentation plan from 13 November 2026.
The manuscript is an unreviewed draft in the American Mathematical Society's article layout.

[Read the compiled paper](llm-graph-selection-paper.pdf).

## Read the model

`main.tex` proves the model's results before applying them to a finite simulation. Its workflow specification determines the next call subject to the dependencies. A proposed release-note study tests the effect of a reviewer's access to the change description.
`references.bib` contains the references. See `reproducibility/verified-sources.md` for the evidence locations.
`presentation-plan.md` specifies the presentation slide by slide, including the spoken script. The editable presentation accompanies this package separately.

The simulation combines three models with three graph families. Sweeping three effort levels at each of two output caps gives 54 configured workflows. Immediate rejection is the 55th candidate.
Every call within a simulated graph uses the chosen model under identical request settings. Node-specific settings remain possible in the general formulation.
Provider documentation supplies admissible settings for model calls, including their token prices. The simulation stipulates complete execution laws from outcomes through token use to duration. Identical laws across providers make the selected provider depend on the tariff comparison.

## Reproduce the results

From the `paper/` directory, use Python 3.12 with the pinned dependencies.

```sh
python -m pip install -r reproducibility/requirements.txt
python reproducibility/simulate_workflows.py
python reproducibility/verify_exact_example.py
python reproducibility/refine_selection.py
python reproducibility/fractional_check.py
```

After dependency installation, the scripts run locally from the supplied simulation laws. Each run overwrites the generated outputs, including the three data figures.
`reproducibility/README.md` explains each file's role in the experiment. The complete conditional rules appear in `reproducibility/simulation-method.md`, including every constant.

## Build the paper

Use a TeX distribution such as TeX Live, including `latexmk` with the packages imported by `main.tex`. MacTeX is suitable on macOS. Run the build from this directory.

```sh
bash check-paper.sh
```

The compiled paper is `build/main.pdf`. The build check rejects LaTeX errors, including unresolved references. Its source checks cover citation integrity. An overfull box also fails validation, as does placeholder text.
After checking the rendered pages, copy `build/main.pdf` to `llm-graph-selection-paper.pdf` to update the version linked above.

The first two figures use the TikZ `graphs` library. Their editable sources are included at full size from the following files, each with a corresponding `.caption.tex` file.

| Figure | Source |
| --- | --- |
| Workflow execution | `figures/workflow-execution.tikz.tex` |
| Workflow topologies | `figures/workflow-topologies.tikz.tex` |

Every figure has a standalone vector PDF. Data figures also have PNG copies for reuse.
The figure specifications explain how the inputs determine the visual marks. Review records retain the layout checks. Project paths are relative to `paper/`, with environment paths identifying the original rendering setup.

## Validation

Independent review covered the finite mixture proof, extending to the calibration bound. Checks also covered the reduced-cost argument, including its approximate gap bound.
Rational arithmetic verifies the small appendix example by matching the cost of a feasible rule to a dual lower bound.
The simulation validates each outcome distribution before checking the selected rules against the service constraints. With known coefficients, the reduced-cost search must reproduce the full-pool optimum.

The main calibration uses outcome counts equivalent to 4.32 million synthetic trials across 216 cells whose coefficients are treated as unknown. The baseline service requirements apply separately in both scenarios.
Additional calculations enforce service limits within each task group. Timing analyses assess sensitivity to accumulated queue delay. Acceptance by the deadline receives a separate calculation.

All cited sources have a retained verification entry. The nine slides from 13 November 2026 were visually reviewed.

The implementation supports conclusions about this finite simulator. A production pilot must measure complete workflows on representative tasks under the intended load. Evaluate the selected mix against the current pipeline, supplemented by a strong single-model baseline under the same service limits.
