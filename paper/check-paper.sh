#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p build
latexmk -pdf -interaction=nonstopmode -halt-on-error -file-line-error -outdir=build main.tex
python3 - <<'PY'
from pathlib import Path
import re
tex = Path('main.tex').read_text()
bib = Path('references.bib').read_text()
log = Path('build/main.log').read_text(errors='replace')
keys = set(re.findall(r'@\w+\s*\{\s*([^,]+),', bib))
cited = {key.strip() for group in re.findall(r'\\cite(?:\[[^]]*\])*\{([^}]+)\}', tex) for key in group.split(',')}
assert cited <= keys, f'Missing bibliography keys: {cited-keys}'
assert not re.search(r'SIMULATIONSECTION|TODO|TBD|PLACEHOLDER', tex)
assert not re.search(r'Overfull \\[hv]box|undefined references|Citation .* undefined|Reference .* undefined', log)
assert len(cited) == 17, f'Expected 17 cited sources, found {len(cited)}'
print('Paper build, citation keys and layout diagnostics passed.')
PY
