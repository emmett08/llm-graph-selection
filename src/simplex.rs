//! Dense two-phase simplex for finite, reasonably scaled LPs.
//!
//! A row is first divided by its largest absolute coefficient. The objective
//! is scaled in the same way. The solver treats values within PIVOT_TOLERANCE
//! of zero as zero during pivots, and checks the returned point in the scaled
//! constraints. Results are floating-point decisions, not exact certificates.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Infeasible,
    Unbounded,
    Numerical,
}

const PIVOT_TOLERANCE: f64 = 1e-10;
const FEASIBILITY_TOLERANCE: f64 = 1e-8;
const ITERATION_LIMIT: usize = 100_000;

/// Maximise `c * x` subject to `a * x <= b` and `x >= 0`.
pub(crate) fn maximise(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> Result<Vec<f64>, Status> {
    if a.len() != b.len() || c.iter().chain(b).any(|value| !value.is_finite()) {
        return Err(Status::Numerical);
    }

    let columns = c.len();
    let mut constraints = Vec::with_capacity(a.len());
    let mut bounds = Vec::with_capacity(b.len());
    for (row, &bound) in a.iter().zip(b) {
        if row.len() != columns || row.iter().any(|value| !value.is_finite()) {
            return Err(Status::Numerical);
        }
        let scale = max_abs(row);
        if scale == 0.0 {
            if bound < 0.0 {
                return Err(Status::Infeasible);
            }
            continue;
        }
        let scaled_bound = bound / scale;
        if !scaled_bound.is_finite() {
            return Err(Status::Numerical);
        }
        constraints.push(row.iter().map(|x| x / scale).collect::<Vec<_>>());
        bounds.push(scaled_bound);
    }

    if constraints.is_empty() {
        return if c.iter().any(|&value| value > 0.0) {
            Err(Status::Unbounded)
        } else {
            Ok(vec![0.0; columns])
        };
    }
    let objective_scale = max_abs(c);
    let objective = if objective_scale == 0.0 {
        vec![0.0; columns]
    } else {
        c.iter().map(|x| x / objective_scale).collect()
    };

    let point = Tableau::new(&constraints, &bounds, &objective).solve()?;
    for (row, &bound) in constraints.iter().zip(&bounds) {
        let value: f64 = row.iter().zip(&point).map(|(a, x)| a * x).sum();
        let allowance = FEASIBILITY_TOLERANCE * bound.abs().max(1.0);
        if !value.is_finite() || value - bound > allowance {
            return Err(Status::Numerical);
        }
    }
    Ok(point)
}

fn max_abs(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(0.0, |largest, value| largest.max(value.abs()))
}

struct Tableau {
    rows: usize,
    columns: usize,
    basic: Vec<usize>,
    nonbasic: Vec<usize>,
    values: Vec<Vec<f64>>,
    pivots: usize,
}

impl Tableau {
    fn new(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> Self {
        let rows = a.len();
        let columns = c.len();
        let artificial = rows + columns;
        let mut values = vec![vec![0.0; columns + 2]; rows + 2];

        for (i, row) in a.iter().enumerate() {
            values[i][..columns].copy_from_slice(row);
            values[i][columns] = -1.0;
            values[i][columns + 1] = b[i];
        }
        for (target, &coefficient) in values[rows].iter_mut().zip(c) {
            *target = -coefficient;
        }
        values[rows + 1][columns] = 1.0;

        let mut nonbasic: Vec<usize> = (0..columns).collect();
        nonbasic.push(artificial);
        Self {
            rows,
            columns,
            basic: (columns..artificial).collect(),
            nonbasic,
            values,
            pivots: 0,
        }
    }

    fn pivot(&mut self, row: usize, column: usize) -> Result<(), Status> {
        if self.pivots >= ITERATION_LIMIT {
            return Err(Status::Numerical);
        }
        self.pivots += 1;
        let inverse = 1.0 / self.values[row][column];
        let pivot_row = self.values[row].clone();
        for (i, values) in self.values.iter_mut().enumerate() {
            if i == row {
                for (j, entry) in values.iter_mut().enumerate() {
                    *entry = if j == column {
                        inverse
                    } else {
                        *entry * inverse
                    };
                }
            } else if values[column] != 0.0 {
                let factor = values[column] * inverse;
                for (j, entry) in values.iter_mut().enumerate() {
                    *entry = if j == column {
                        -factor
                    } else {
                        *entry - pivot_row[j] * factor
                    };
                }
            }
            if values.iter().any(|value| !value.is_finite()) {
                return Err(Status::Numerical);
            }
        }
        std::mem::swap(&mut self.basic[row], &mut self.nonbasic[column]);
        Ok(())
    }

    fn optimise(&mut self, phase_one: bool) -> Result<(), Status> {
        let objective = self.rows + usize::from(phase_one);
        let artificial = self.rows + self.columns;
        let rhs = self.columns + 1;
        loop {
            // Bland's rule for entering and tied leaving variables makes
            // degenerate pivots deterministic and avoids cycling in exact arithmetic.
            let entering = (0..=self.columns)
                .filter(|&j| {
                    (phase_one || self.nonbasic[j] != artificial)
                        && self.values[objective][j] < -PIVOT_TOLERANCE
                })
                .min_by_key(|&j| self.nonbasic[j]);
            let Some(column) = entering else {
                return Ok(());
            };

            let mut leaving: Option<(usize, f64)> = None;
            for i in 0..self.rows {
                let coefficient = self.values[i][column];
                if coefficient <= PIVOT_TOLERANCE {
                    continue;
                }
                let ratio = self.values[i][rhs].max(0.0) / coefficient;
                if !ratio.is_finite() {
                    return Err(Status::Numerical);
                }
                let better = match leaving {
                    Some((best, best_ratio)) => {
                        ratio < best_ratio
                            || (ratio == best_ratio && self.basic[i] < self.basic[best])
                    }
                    None => true,
                };
                if better {
                    leaving = Some((i, ratio));
                }
            }
            let Some((row, _)) = leaving else {
                // A positive coefficient below the pivot tolerance may still
                // bound the feasible ray. Its effect is unresolved here.
                let nearly_blocked = (0..self.rows).any(|i| self.values[i][column] > 0.0);
                return Err(if nearly_blocked {
                    Status::Numerical
                } else {
                    Status::Unbounded
                });
            };
            self.pivot(row, column)?;
        }
    }

    fn solve(mut self) -> Result<Vec<f64>, Status> {
        let rhs = self.columns + 1;
        let lowest_row = (0..self.rows)
            .min_by(|&left, &right| self.values[left][rhs].total_cmp(&self.values[right][rhs]))
            .ok_or(Status::Numerical)?;

        if self.values[lowest_row][rhs] < -PIVOT_TOLERANCE {
            self.pivot(lowest_row, self.columns)?;
            self.optimise(true).map_err(|_| Status::Numerical)?;

            let feasibility = self.values[self.rows + 1][rhs];
            if feasibility < -PIVOT_TOLERANCE {
                return Err(Status::Infeasible);
            }
            if feasibility > PIVOT_TOLERANCE {
                return Err(Status::Numerical);
            }

            // Remove a zero-valued artificial basic variable if its row has
            // a usable coefficient. A row with no usable coefficient is redundant.
            let artificial = self.rows + self.columns;
            if let Some(row) = self
                .basic
                .iter()
                .position(|&variable| variable == artificial)
            {
                let replacement = (0..=self.columns)
                    .filter(|&j| self.values[row][j].abs() > PIVOT_TOLERANCE)
                    .min_by_key(|&j| self.nonbasic[j]);
                if let Some(column) = replacement {
                    self.pivot(row, column)?;
                }
            }
        }

        self.optimise(false)?;
        let mut point = vec![0.0; self.columns];
        for (row, &variable) in self.basic.iter().enumerate() {
            let value = self.values[row][rhs];
            if value < -FEASIBILITY_TOLERANCE {
                return Err(Status::Numerical);
            }
            if variable < self.columns {
                point[variable] = value.max(0.0);
            }
        }
        Ok(point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_rhs_and_equality() {
        let a = vec![vec![-1.0, -1.0], vec![1.0, 1.0], vec![1.0, 0.0]];
        let point = maximise(&a, &[-1.0, 1.0, 0.25], &[2.0, 1.0]).unwrap();
        assert!((point[0] - 0.25).abs() < 1e-10);
        assert!((point[1] - 0.75).abs() < 1e-10);
    }

    #[test]
    fn infeasible_lower_bound() {
        let a = vec![vec![1.0], vec![-1.0]];
        assert_eq!(maximise(&a, &[0.0, -1.0], &[1.0]), Err(Status::Infeasible));
    }

    #[test]
    fn unbounded_after_feasibility_phase() {
        let a = vec![vec![-1.0]];
        assert_eq!(maximise(&a, &[-1.0], &[1.0]), Err(Status::Unbounded));
    }

    #[test]
    fn works_with_tiny_row_and_objective_scales() {
        let a = vec![vec![1e-40]];
        assert_eq!(maximise(&a, &[5e-40], &[1e-90]).unwrap(), vec![5.0]);
    }

    #[test]
    fn almost_zero_blocking_pivot_is_not_declared_unbounded() {
        let a = vec![vec![1.0, 1e-14]];
        assert_eq!(maximise(&a, &[1.0], &[0.0, 1.0]), Err(Status::Numerical));
    }

    #[test]
    fn empty_constraints_and_invalid_data() {
        assert_eq!(maximise(&[], &[], &[-1.0]).unwrap(), vec![0.0]);
        assert_eq!(maximise(&[], &[], &[1e-50]), Err(Status::Unbounded));
        assert_eq!(
            maximise(&[vec![0.0]], &[-1.0], &[1.0]),
            Err(Status::Infeasible)
        );
        assert_eq!(
            maximise(&[vec![f64::NAN]], &[0.0], &[1.0]),
            Err(Status::Numerical)
        );
        assert_eq!(maximise(&[vec![]], &[0.0], &[1.0]), Err(Status::Numerical));
    }

    #[test]
    fn bland_rule_resolves_cycling_example() {
        let a = vec![
            vec![0.5, -5.5, -2.5, 9.0],
            vec![0.5, -1.5, -0.5, 1.0],
            vec![1.0, 0.0, 0.0, 0.0],
        ];
        let point = maximise(&a, &[0.0, 0.0, 1.0], &[10.0, -57.0, -9.0, -24.0]).unwrap();
        assert!((point[0] - 1.0).abs() < 1e-9);
        assert!((point[2] - 1.0).abs() < 1e-9);
        assert!(point[1].abs() < 1e-9);
        assert!(point[3].abs() < 1e-9);
    }
}
