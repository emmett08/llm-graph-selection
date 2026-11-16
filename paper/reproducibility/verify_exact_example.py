"""Verify a synthetic graph-candidate extension of the supplied LP."""
from fractions import Fraction as F
import json
import numpy as np
from scipy.optimize import linprog

# Whole-workflow coefficients chosen for the arithmetic example.
names = ['single_fast', 'serial_review', 'reject', 'parallel_review']
a = [F(1), F(4,5), F(0), F(9,10)]
b = [F(1,5), F(1,25), F(0), F(9,200)]
c = [F(1), F(3), F(0), F(11,5)]
late = [F(1,10), F(1,20), F(0), F(3,100)]
bad_b = [F(6,5)*value for value in b]
epsilon, gamma, delta = F(1,10), F(3,5), F(2,25)

def dot(left, right):
    return sum((x*y for x,y in zip(left,right)), F(0))

def solve(candidate_count):
    aa, bb, cc, ll, adverse = [np.array(list(map(float, values[:candidate_count]))) for values in (a,b,c,late,bad_b)]
    rows = np.vstack([bb-float(epsilon)*aa, adverse-float(epsilon)*aa, -aa, ll])
    result = linprog(cc, A_ub=rows, b_ub=[0,0,-float(gamma),float(delta)], A_eq=np.ones((1,candidate_count)), b_eq=[1], bounds=(0,None), method='highs')
    if not result.success:
        raise RuntimeError(result.message)
    return result

initial = solve(3)
expanded = solve(4)
expected = [F(2,15), F(0), F(47,135), F(14,27)]
assert np.allclose(expanded.x, list(map(float, expected)), atol=1e-9, rtol=0)
assert sum(expected) == 1 and min(expected) >= 0
assert dot(a,expected) == gamma
assert dot(bad_b,expected) / dot(a,expected) == epsilon
assert dot(late,expected) == F(13,450) <= delta
assert dot(c,expected) == F(172,135)

# A rational dual witness proves a matching lower bound for every feasible mix.
beta, kappa = F(650,81), F(172,81)
slacks = [cc + beta*(bb-epsilon*aa) - kappa*aa for aa,bb,cc in zip(a,bad_b,c)]
assert min(slacks) >= 0
assert kappa*gamma == F(172,135)
assert dot(slacks,expected) == 0

# Reduced cost using exact initial dual multipliers.
initial_beta, initial_kappa = F(275,18), F(113,36)
reduced = c[3] + initial_beta*(bad_b[3]-epsilon*a[3]) - initial_kappa*a[3]
assert reduced == -F(47,40)
assert np.isclose(initial.fun, float(F(113,60)))
assert np.isclose(expanded.fun, float(F(172,135)))

result = dict(scope='Synthetic coefficients; no live LLM calls', initial_cost=str(F(113,60)), expanded_cost=str(F(172,135)), weights=dict(zip(names,map(str,expected))), completion=str(dot(a,expected)), worst_defect_rate=str(dot(bad_b,expected)/dot(a,expected)), late=str(dot(late,expected)), new_column_reduced_cost=str(reduced), exact_dual_slacks=list(map(str,slacks)), all_assertions_passed=True)
print(json.dumps(result,indent=2))
