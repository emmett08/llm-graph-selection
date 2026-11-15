# Selecting LLM workflow graphs and reasoning settings

This package contains the manuscript revision of 15 November 2026 and the presentation plan from 13 November 2026.
The manuscript is an unreviewed draft in the American Mathematical Society's article layout.

[Read the compiled paper](llm-graph-selection-paper.pdf).

## Read the model

`main.tex` develops the model and its proofs, then applies it to a finite simulation. It explains how call dependencies constrain execution and how the workflow chooses the next call. A proposed release-note study tests the effect of a reviewer's access to the change description.
`references.bib` contains the references. See `reproducibility/verified-sources.md` for the evidence locations.
`presentation-plan.md` gives each slide's headline with its layout and presenter script. The editable presentation accompanies this package separately.

The simulation combines three models with three graph families. Each combination has three effort levels and two output caps, giving 54 configured workflows. Immediate rejection is the 55th candidate.
All calls within each simulated graph share the chosen model and settings. The general formulation permits different settings at each node.
Provider documentation supplies the admissible settings and token prices. The simulation stipulates the outcome distributions and the rules for token use and duration. Every provider uses the same execution laws, so the tariff comparison determines the selected provider.

## Reproduce the figures and arithmetic

From the `paper/` directory, use Python 3.12 and the pinned dependencies.

```sh
python -m pip install -r reproducibility/requirements.txt
python reproducibility/simulate_workflows.py
python reproducibility/verify_exact_example.py
python reproducibility/refine_selection.py
python reproducibility/fractional_check.py
```

After dependencies are installed, the scripts run locally from the supplied simulation laws. The simulation rewrites its output CSV/JSON files and the three data figures.
`reproducibility/README.md` maps those files and explains what each sample and cost measure includes. `reproducibility/simulation-method.md` gives all constants and conditional outcome rules.

## Build the paper

Install TeX Live or MacTeX with `latexmk` and the packages imported by `main.tex`, then run from this directory.

```sh
bash check-paper.sh
```

The compiled paper is `build/main.pdf`. The script checks the build log for LaTeX errors and unresolved references. It rejects overfull boxes and checks the source for missing citation keys or placeholder text.
After checking the rendered pages, copy `build/main.pdf` to `llm-graph-selection-paper.pdf` to update the version linked above.

Figures 1 and 2 are editable directed graphs drawn with the TikZ `graphs` library. The manuscript includes `figures/workflow-execution.tikz.tex` and `figures/workflow-topologies.tikz.tex` at full size. Each has a separate `.caption.tex` file. The remaining figures are vector PDFs with PNG copies for reuse.
Each new figure has a standalone vector PDF. The figure specifications identify the inputs and mark meanings. The audit and review records retain the final checks and compared layouts. Their project file paths are relative to `paper/`. Environment paths in the trace identify the original rendering setup.

## Evidence and checks

The finite mixture proof and the calibration bound were independently reviewed. The reduced-cost formula and approximate gap bound were also checked.
Rational arithmetic verifies the small appendix example by matching the cost of a feasible rule to a dual lower bound.
The simulation checks that its outcome distributions are valid and that the selected routing rules meet the constraints. It also checks agreement between full-pool optimisation and reduced-cost search using known coefficients.

The main calibration uses outcome counts equivalent to 4.32 million synthetic trials across 216 cells whose coefficients are treated as unknown. The baseline service requirements apply separately in both scenarios.
Additional calculations enforce service limits within each task group. Timing analyses examine acceptance by the deadline and the effect of accumulated queue delay.

All cited sources have a retained verification entry. The nine slides from 13 November 2026 were visually reviewed.

The implementation supports conclusions about this finite simulator. A production pilot must measure complete workflows on representative tasks under the intended load. That pilot should compare the selected mix with the current pipeline and a strong single-model baseline, using the same service limits.
