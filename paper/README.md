# Selecting LLM workflow graphs and reasoning settings

This package contains the mathematical revision of 14 November 2026 and the presentation plan from 13 November 2026.
The manuscript uses a neutral AMS article layout. It has no target-journal formatting or peer-review status.

[Read the compiled paper](llm-graph-selection-paper.pdf).

## Read the model

`main.tex` is the complete manuscript. It defines executable graphs with model-specific settings, derives the constrained selection programme, proves the calibration and pricing results, and reports the finite simulation.
`references.bib` contains 17 references checked against primary sources. The evidence locations and access limits are in `reproducibility/verified-sources.md`.
`presentation-plan.md` contains the exact slide headlines, layouts and presenter script. The editable presentation accompanies this package separately.

The simulation contains 55 candidates formed from three documented model identities, three graph families, three effort levels and two output caps, plus rejection.
All nodes within each simulated graph share the chosen model and settings. The general formulation permits different settings at each node.
Only the configuration domains and token prices come from provider documentation. The task outcomes, token counts and durations are stipulated. Provider response laws are equal, so the selected provider follows the tariff comparison by construction.

## Reproduce the figures and arithmetic

From the `paper/` directory, use Python 3.12 and the pinned dependencies.

```sh
python -m pip install -r reproducibility/requirements.txt
python reproducibility/simulate_workflows.py
python reproducibility/verify_exact_example.py
python reproducibility/refine_selection.py
python reproducibility/fractional_check.py
```

The scripts need no API key or network access after dependencies are installed. The simulation rewrites its output CSV/JSON files and the three data figures. `reproducibility/README.md` maps those files and explains the sampling and cost boundaries. `reproducibility/simulation-method.md` gives all constants and conditional outcome rules.

## Build the paper

Install TeX Live or MacTeX with `latexmk` and the packages imported by `main.tex`, then run from this directory.

```sh
bash check-paper.sh
```

The compiled paper is `build/main.pdf`. The script fails on a LaTeX error, unresolved citation or reference, or an overfull box. It also checks citation keys and placeholder text.

Figures 1 and 2 are editable directed graphs drawn with the TikZ `graphs` library. The manuscript includes `figures/workflow-execution.tikz.tex` and `figures/workflow-topologies.tikz.tex` at full size. Each has a separate `.caption.tex` file. The remaining figures are vector PDFs with PNG copies for reuse.
Each new figure has a standalone vector PDF. The figure specifications identify the inputs and mark meanings. The audit and review records retain the final checks and compared layouts. Their project file paths are relative to `paper/`. Environment paths in the trace identify the original rendering setup.

## Evidence and checks

The finite mixture proof and the calibration bound were independently reviewed. The reduced-cost formula and approximate gap bound were also checked. Rational arithmetic verifies the small appendix example, including a matching dual bound. The simulation checks probability totals, accepted-defect bounds, routing feasibility and equality between full-pool selection and exact pricing search.

The main calibration uses 4.32 million synthetic trials across 216 uncertain cells. The baseline service requirements apply separately in both scenarios. The revision also solves for per-group service and evaluates timely completion and accumulated queue delay.

All cited sources have a retained verification entry. The nine slides from 13 November 2026 were visually reviewed.

The implementation supports conclusions about this finite simulator. A production pilot must measure complete workflows on representative tasks under the intended load. That pilot should compare the selected mix with the current pipeline and a strong single-model baseline, using the same service limits.
