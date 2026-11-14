"""One-sided calibration of paired conditional-defect coefficients."""

from __future__ import annotations

import numpy as np


def paired_risk_upper(a_count, b_count, n, epsilon, failure_probability):
    """Bound E[Y - epsilon*A] from scalar or broadcastable counts.

    Each observation pairs the acceptance indicator A with the accepted-defect
    indicator Y <= A. The input b_count counts accepted defects, a_count counts
    all acceptances, and n counts all calibration observations. For i.i.d.
    observations and fixed epsilon, coverage is at least 1 - failure_probability
    (Maurer and Pontil, 2009, Theorem 4). The caller allocates this probability
    across any family of bounds used together. With fewer than two observations,
    the result is the upper endpoint of the outcome range, 1 - epsilon.
    """
    accepted = np.asarray(a_count, dtype=float)
    defective = np.asarray(b_count, dtype=float)
    total = np.asarray(n, dtype=float)
    epsilon = np.asarray(epsilon, dtype=float)
    failure_probability = np.asarray(failure_probability, dtype=float)

    for count in (accepted, defective, total):
        if not np.all(np.isfinite(count) & (count == np.floor(count))):
            raise ValueError("Counts must be finite integers")
    if np.any((defective < 0) | (defective > accepted) | (accepted > total)):
        raise ValueError("Counts must satisfy 0 <= b_count <= a_count <= n")
    if not np.all((0 <= epsilon) & (epsilon <= 1)):
        raise ValueError("epsilon must lie in [0, 1]")
    if not np.all((0 < failure_probability) & (failure_probability < 1)):
        raise ValueError("failure_probability must lie in (0, 1)")

    accepted_clean = accepted - defective
    rejected = total - accepted
    safe_total = np.maximum(total, 1)
    degrees_of_freedom = np.maximum(total - 1, 1)
    upper_endpoint = 1 - epsilon
    mean = (defective * upper_endpoint - accepted_clean * epsilon) / safe_total

    # Each unordered pair from different categories contributes its squared gap.
    variance = (
        defective / safe_total * (accepted_clean / degrees_of_freedom)
        + rejected / safe_total
        * (
            accepted_clean / degrees_of_freedom * epsilon**2
            + defective / degrees_of_freedom * upper_endpoint**2
        )
    )
    log_term = np.log(2) - np.log(failure_probability)
    radius = np.sqrt(2 * variance * log_term / safe_total)
    radius += 7 * log_term / (3 * degrees_of_freedom)
    upper = np.where(total >= 2, np.minimum(upper_endpoint, mean + radius), upper_endpoint)
    return upper[()]
