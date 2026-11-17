//! Simultaneous cell bounds for routing among a frozen set of workflows.
//!
//! Freeze the candidate set, task classifier, sampling plan and method before
//! collecting calibration observations. Use independent representative draws
//! within each cell. The union bound allows dependence between cells.
//! Paired methods also require a fixed defect tolerance before calibration.
//! The supplied cost cap must hold for every attempt in the cell.

use crate::{Cell, Error, Moments, OutcomeMeans, RiskBound};

/// One attempted workflow execution, including any rejection or lateness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttemptSample {
    /// The workflow released an accepted output.
    pub accepted: bool,
    /// The accepted output had a material defect. This implies `accepted`.
    pub defective: bool,
    /// Total incurred cost, including costs on unsuccessful attempts.
    pub cost: f64,
    /// The attempt exceeded the configured deadline.
    pub late: bool,
}

/// Sufficient statistics for a group–workflow–scenario cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationCell {
    pub attempts: u64,
    pub accepted: u64,
    pub accepted_defects: u64,
    pub late: u64,
    pub total_cost: f64,
    /// A known upper bound on the cost of any attempt in this cell.
    pub cost_cap: f64,
}

/// Sufficient statistics for five outcome means, including task failure.
/// Count failures across all attempts using the service's failure criterion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FailureCalibrationCell {
    pub summary: CalibrationCell,
    pub failed: u64,
}

impl FailureCalibrationCell {
    /// Summarise attempts when success requires timely, defect-free acceptance.
    pub fn from_attempts(attempts: &[AttemptSample], cost_cap: f64) -> Result<Self, Error> {
        let summary = CalibrationCell::from_attempts(attempts, cost_cap)?;
        let failed = attempts
            .iter()
            .filter(|attempt| !attempt.accepted || attempt.defective || attempt.late)
            .count() as u64;
        Ok(Self { summary, failed })
    }

    /// Bound all five means in both directions. Use the same family error and
    /// total number of uncertain cells for every cell. Each of the ten
    /// one-sided bounds uses error probability alpha / (10 * uncertain_cells).
    pub fn calibrate(&self, alpha: f64, uncertain_cells: usize) -> Result<Cell, Error> {
        let summary = self.summary;
        summary.validate()?;
        if self.failed > summary.attempts {
            return Err(invalid("failed attempts exceed the attempt count"));
        }
        let n = summary.attempts as f64;
        let mean = OutcomeMeans {
            moments: Moments {
                acceptance: summary.accepted as f64 / n,
                joint_defect: summary.accepted_defects as f64 / n,
                cost: summary.total_cost / n,
                lateness: summary.late as f64 / n,
            },
            failure: self.failed as f64 / n,
        };
        let (lower, upper) = calibrate_mean_endpoints(
            mean,
            summary.attempts,
            summary.cost_cap,
            alpha,
            uncertain_cells,
        )?;
        Ok(Cell::Interval { lower, upper })
    }
}

/// Bound either attempt averages or averages of independently repeated
/// workload means. In the latter case `observations` counts workloads.
pub(crate) fn calibrate_mean_endpoints(
    mean: OutcomeMeans,
    observations: u64,
    cost_cap: f64,
    alpha: f64,
    uncertain_cells: usize,
) -> Result<(OutcomeMeans, OutcomeMeans), Error> {
    Cell::PointWithFailure(mean).validate()?;
    if observations == 0 {
        return Err(invalid("calibration requires at least one observation"));
    }
    let cost = mean.moments.cost;
    if !cost_cap.is_finite() || cost_cap < 0.0 || cost - cost_cap > 8.0 * f64::EPSILON * cost {
        return Err(invalid(
            "mean cost must not exceed a finite non-negative cost cap",
        ));
    }
    let radius =
        (log_inverse_failure(alpha, uncertain_cells, 10.0)? / (2.0 * observations as f64)).sqrt();
    let endpoint = |direction: f64| {
        let proportion = |value: f64| (value + direction * radius).clamp(0.0, 1.0);
        OutcomeMeans {
            moments: Moments {
                acceptance: proportion(mean.moments.acceptance),
                joint_defect: proportion(mean.moments.joint_defect),
                cost: (cost.min(cost_cap) + direction * cost_cap * radius).clamp(0.0, cost_cap),
                lateness: proportion(mean.moments.lateness),
            },
            failure: proportion(mean.failure),
        }
    };
    Ok((endpoint(-1.0), endpoint(1.0)))
}

fn log_inverse_failure(
    alpha: f64,
    uncertain_cells: usize,
    bounds_per_cell: f64,
) -> Result<f64, Error> {
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(invalid(
            "family error alpha must be strictly between zero and one",
        ));
    }
    if uncertain_cells == 0 {
        return Err(invalid("the uncertain cell count must be positive"));
    }
    Ok(bounds_per_cell.ln() + (uncertain_cells as f64).ln() - alpha.ln())
}

/// Prespecified construction for the conditional defect constraint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CalibrationMethod {
    /// Bound acceptance and accepted defects separately.
    SeparateHoeffding,
    /// Bound the mean of the paired score with a range bound.
    PairedHoeffding { epsilon: f64 },
    /// Bound the mean of the paired score using its sample variance.
    PairedEmpiricalBernstein { epsilon: f64 },
}

/// Cell endpoints for a conservative linear programme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibratedCell {
    /// Lower completion and upper joint defect, cost and lateness bounds.
    ///
    /// With a paired method the joint defect field is the structural upper
    /// bound one. The programme uses the tighter signed coefficient in `risk`.
    /// These are bounds on distinct quantities and need not obey
    /// `joint_defect <= acceptance`.
    pub conservative: Moments,
    /// Upper bound on the mean of `Y - epsilon * A`, when supplied.
    pub risk: Option<RiskBound>,
}

impl CalibrationCell {
    /// Summarise raw attempts and check that all observed costs fit the cap.
    ///
    /// The observations alone cannot establish that this cap holds in the
    /// population. Establish it from the workflow and billing rules.
    pub fn from_attempts(attempts: &[AttemptSample], cost_cap: f64) -> Result<Self, Error> {
        if !cost_cap.is_finite() || cost_cap < 0.0 {
            return Err(invalid("cost cap must be finite and non-negative"));
        }
        let attempts_count =
            u64::try_from(attempts.len()).map_err(|_| invalid("attempt count exceeds u64"))?;
        let mut cell = Self {
            attempts: attempts_count,
            accepted: 0,
            accepted_defects: 0,
            late: 0,
            total_cost: 0.0,
            cost_cap,
        };
        let mut compensation = 0.0;
        for sample in attempts {
            if sample.defective && !sample.accepted {
                return Err(invalid("an accepted defect requires an accepted output"));
            }
            if !sample.cost.is_finite() || sample.cost < 0.0 || sample.cost > cost_cap {
                return Err(invalid(
                    "attempt cost must lie between zero and the cost cap",
                ));
            }
            cell.accepted += u64::from(sample.accepted);
            cell.accepted_defects += u64::from(sample.defective);
            cell.late += u64::from(sample.late);

            // Compensated summation limits accumulated rounding error.
            let corrected = sample.cost - compensation;
            let updated = cell.total_cost + corrected;
            compensation = (updated - cell.total_cost) - corrected;
            cell.total_cost = updated;
        }
        cell.validate()?;
        Ok(cell)
    }

    /// Construct one cell's simultaneous confidence bounds.
    ///
    /// Use the same family error `alpha`, total count of uncertain cells and
    /// prespecified method for every calibrated cell in the deployment problem.
    /// Each cell uses `alpha / (4 * uncertain_cells)` for each of its four
    /// one-sided bounds. Known coefficients consume no probability budget.
    pub fn calibrate(
        &self,
        alpha: f64,
        uncertain_cells: usize,
        method: CalibrationMethod,
    ) -> Result<CalibratedCell, Error> {
        self.validate()?;
        let n = self.attempts as f64;
        let acceptance = self.accepted as f64 / n;
        let joint_defect = self.accepted_defects as f64 / n;
        let mean_cost = (self.total_cost / n).min(self.cost_cap);
        let log_inverse_failure = log_inverse_failure(alpha, uncertain_cells, 4.0)?;
        let radius = (log_inverse_failure / (2.0 * n)).sqrt();

        let risk = match method {
            CalibrationMethod::SeparateHoeffding => None,
            CalibrationMethod::PairedHoeffding { epsilon } => {
                let mean = self.paired_mean(epsilon)?;
                Some(RiskBound {
                    epsilon,
                    upper: (mean + radius).min(1.0 - epsilon),
                })
            }
            CalibrationMethod::PairedEmpiricalBernstein { epsilon } => {
                let mean = self.paired_mean(epsilon)?;
                let upper = if self.attempts < 2 {
                    1.0 - epsilon
                } else {
                    let variance = self.paired_sample_variance(epsilon);
                    let log_term = 2.0_f64.ln() + log_inverse_failure;
                    let empirical_radius =
                        (2.0 * variance * log_term / n).sqrt() + 7.0 * log_term / (3.0 * (n - 1.0));
                    (mean + empirical_radius).min(1.0 - epsilon)
                };
                Some(RiskBound { epsilon, upper })
            }
        };
        Ok(CalibratedCell {
            conservative: Moments {
                acceptance: (acceptance - radius).max(0.0),
                joint_defect: if risk.is_some() {
                    1.0
                } else {
                    (joint_defect + radius).min(1.0)
                },
                cost: (mean_cost + self.cost_cap * radius).min(self.cost_cap),
                lateness: (self.late as f64 / n + radius).min(1.0),
            },
            risk,
        })
    }

    fn validate(&self) -> Result<(), Error> {
        if self.attempts == 0 {
            return Err(invalid("calibration requires at least one attempt"));
        }
        if self.accepted_defects > self.accepted
            || self.accepted > self.attempts
            || self.late > self.attempts
        {
            return Err(invalid(
                "outcome counts are inconsistent with the attempt count",
            ));
        }
        if !self.cost_cap.is_finite()
            || self.cost_cap < 0.0
            || !self.total_cost.is_finite()
            || self.total_cost < 0.0
        {
            return Err(invalid(
                "cost cap and total cost must be finite and non-negative",
            ));
        }

        let mean = self.total_cost / self.attempts as f64;
        // Allow the final few ulps of accumulated floating-point arithmetic.
        if mean - self.cost_cap > 8.0 * f64::EPSILON * mean {
            return Err(invalid("mean observed cost exceeds the cost cap"));
        }
        Ok(())
    }

    fn paired_mean(&self, epsilon: f64) -> Result<f64, Error> {
        if !probability(epsilon) {
            return Err(invalid(
                "defect tolerance epsilon must lie between zero and one",
            ));
        }
        let defective = self.accepted_defects as f64 / self.attempts as f64;
        let clean = (self.accepted - self.accepted_defects) as f64 / self.attempts as f64;
        Ok(defective * (1.0 - epsilon) - clean * epsilon)
    }

    fn paired_sample_variance(&self, epsilon: f64) -> f64 {
        let n = self.attempts as f64;
        let defective = self.accepted_defects as f64 / n;
        let clean = (self.accepted - self.accepted_defects) as f64 / n;
        let rejected = (self.attempts - self.accepted) as f64 / n;

        // Count pairs from distinct outcome classes by their squared score gaps.
        let pair_sum = defective * clean
            + rejected * (clean * epsilon.powi(2) + defective * (1.0 - epsilon).powi(2));
        pair_sum * n / (n - 1.0)
    }
}

impl CalibratedCell {
    /// Take exact cell moments when the outcome law is known by construction.
    pub fn known(moments: Moments) -> Result<Self, Error> {
        if !probability(moments.acceptance)
            || !probability(moments.joint_defect)
            || !probability(moments.lateness)
            || moments.joint_defect > moments.acceptance
            || !moments.cost.is_finite()
            || moments.cost < 0.0
        {
            return Err(invalid("invalid known moments"));
        }
        Ok(Self {
            conservative: moments,
            risk: None,
        })
    }

    /// Exact cell for immediate rejection with zero cost and no queueing.
    pub fn immediate_reject() -> Self {
        Self {
            conservative: Moments {
                acceptance: 0.0,
                joint_defect: 0.0,
                cost: 0.0,
                lateness: 0.0,
            },
            risk: None,
        }
    }
}

fn probability(value: f64) -> bool {
    (0.0..=1.0).contains(&value)
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrates_five_means_with_ten_bounds_per_uncertain_cell() {
        let tally = FailureCalibrationCell {
            summary: CalibrationCell {
                attempts: 20_000,
                accepted: 16_000,
                accepted_defects: 1_000,
                late: 2_000,
                total_cost: 4_000.0,
                cost_cap: 0.5,
            },
            failed: 5_000,
        };
        let Cell::Interval { lower, upper } = tally.calibrate(0.05, 7).unwrap() else {
            panic!("expected two-sided bounds");
        };
        let radius = ((10.0_f64 * 7.0 / 0.05).ln() / (2.0 * 20_000.0)).sqrt();
        assert!((lower.failure - (0.25 - radius)).abs() < 1e-12);
        assert!((upper.failure - (0.25 + radius)).abs() < 1e-12);
        assert!((lower.moments.cost - (0.2 - 0.5 * radius)).abs() < 1e-12);
        assert!((upper.moments.acceptance - (0.8 + radius)).abs() < 1e-12);
        assert!(FailureCalibrationCell {
            failed: 20_001,
            ..tally
        }
        .calibrate(0.05, 7)
        .is_err());
    }

    #[test]
    fn tallies_joint_failure_from_attempts() {
        let clean = AttemptSample {
            accepted: true,
            defective: false,
            cost: 0.0,
            late: false,
        };
        let attempts = [
            clean,
            AttemptSample {
                defective: true,
                late: true,
                ..clean
            },
            AttemptSample {
                accepted: false,
                late: true,
                ..clean
            },
        ];
        let tally = FailureCalibrationCell::from_attempts(&attempts, 0.0).unwrap();
        assert_eq!(tally.failed, 2);
        assert_eq!(tally.summary.late, 2);
        assert_eq!(tally.summary.accepted_defects, 1);
    }

    #[test]
    fn raw_attempts_retain_counts_and_rejection_costs() {
        let cell = CalibrationCell::from_attempts(
            &[
                AttemptSample {
                    accepted: true,
                    defective: true,
                    cost: 0.2,
                    late: false,
                },
                AttemptSample {
                    accepted: true,
                    defective: false,
                    cost: 0.1,
                    late: true,
                },
                AttemptSample {
                    accepted: false,
                    defective: false,
                    cost: 0.5,
                    late: true,
                },
            ],
            0.5,
        )
        .unwrap();
        assert_eq!(cell.attempts, 3);
        assert_eq!(cell.accepted, 2);
        assert_eq!(cell.accepted_defects, 1);
        assert_eq!(cell.late, 2);
        assert!((cell.total_cost - 0.8).abs() < 1e-12);
    }

    #[test]
    fn paired_bounds_agree_with_expanded_outcomes() {
        let epsilon = 0.1;
        let alpha = 0.05;
        let counts = 200;
        let cell = CalibrationCell {
            attempts: 20_000,
            accepted: 18_000,
            accepted_defects: 200,
            late: 1_100,
            total_cost: 800.0,
            cost_cap: 0.12,
        };
        let outcomes = [(200, 1.0 - epsilon), (17_800, -epsilon), (2_000, 0.0)];
        let n = cell.attempts as f64;
        let mu = outcomes
            .iter()
            .map(|&(count, score)| (count as f64) * score)
            .sum::<f64>()
            / n;
        let variance = outcomes
            .iter()
            .flat_map(|&(count, score)| std::iter::repeat_n(score, count))
            .map(|score| (score - mu).powi(2))
            .sum::<f64>()
            / (n - 1.0);
        let log = (4.0 * counts as f64 / alpha).ln();
        let radius = (log / (2.0 * n)).sqrt();
        let expected = mu
            + (2.0 * variance * (2.0_f64.ln() + log) / n).sqrt()
            + 7.0 * (2.0_f64.ln() + log) / (3.0 * (n - 1.0));

        let separate = cell
            .calibrate(alpha, counts, CalibrationMethod::SeparateHoeffding)
            .unwrap();
        let paired = cell
            .calibrate(
                alpha,
                counts,
                CalibrationMethod::PairedHoeffding { epsilon },
            )
            .unwrap();
        let empirical = cell
            .calibrate(
                alpha,
                counts,
                CalibrationMethod::PairedEmpiricalBernstein { epsilon },
            )
            .unwrap();
        assert!(separate.risk.is_none());
        assert!((separate.conservative.joint_defect - (0.01 + radius)).abs() < 1e-12);
        assert!((separate.conservative.acceptance - (0.9 - radius)).abs() < 1e-12);
        assert!((separate.conservative.cost - (0.04 + 0.12 * radius)).abs() < 1e-12);
        assert!((separate.conservative.lateness - (0.055 + radius)).abs() < 1e-12);

        let paired_upper = paired.risk.unwrap().upper;
        assert!((paired_upper - (mu + radius)).abs() < 1e-12);
        assert!(
            paired_upper
                <= separate.conservative.joint_defect - epsilon * separate.conservative.acceptance
        );
        assert!((empirical.risk.unwrap().upper - expected).abs() < 1e-12);
        assert!(expected < paired_upper);
    }

    #[test]
    fn small_samples_and_known_rejection_use_their_structural_bounds() {
        let from_sample = CalibrationCell::from_attempts(
            &[AttemptSample {
                accepted: false,
                defective: false,
                cost: 0.0,
                late: false,
            }],
            0.0,
        )
        .unwrap()
        .calibrate(
            0.05,
            1,
            CalibrationMethod::PairedEmpiricalBernstein { epsilon: 0.2 },
        )
        .unwrap();
        assert_eq!(from_sample.risk.unwrap().upper, 0.8);
        assert_eq!(from_sample.conservative.cost, 0.0);

        let rejection = CalibratedCell::immediate_reject();
        assert_eq!(rejection.conservative.acceptance, 0.0);
        assert_eq!(rejection.conservative.joint_defect, 0.0);
        assert!(rejection.risk.is_none());
    }

    #[test]
    fn invalid_outcomes_fail_before_calibration() {
        let invalid = AttemptSample {
            accepted: false,
            defective: true,
            cost: 0.0,
            late: false,
        };
        assert!(CalibrationCell::from_attempts(&[invalid], 0.0).is_err());
        let invalid = AttemptSample {
            defective: false,
            cost: 1.5,
            ..invalid
        };
        assert!(CalibrationCell::from_attempts(&[invalid], 1.0).is_err());
        assert!(CalibrationCell::from_attempts(&[], 1.0).is_err());

        let cell = CalibrationCell::from_attempts(&[invalid], 2.0).unwrap();
        assert!(cell
            .calibrate(1.0, 1, CalibrationMethod::SeparateHoeffding)
            .is_err());
        assert!(cell
            .calibrate(0.05, 0, CalibrationMethod::SeparateHoeffding)
            .is_err());
        assert!(cell
            .calibrate(
                0.05,
                1,
                CalibrationMethod::PairedHoeffding { epsilon: f64::NAN }
            )
            .is_err());
        assert!(CalibrationCell {
            accepted: 2,
            ..cell
        }
        .calibrate(0.05, 1, CalibrationMethod::SeparateHoeffding)
        .is_err());
    }
}
