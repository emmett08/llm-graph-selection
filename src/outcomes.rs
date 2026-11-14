//! Build cell coefficients from a finite distribution over complete executions.

use crate::{probability, Cell, Error, Moments};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalOutcome {
    pub probability: f64,
    pub accepted: bool,
    /// A material defect in an accepted output. Rejected attempts use false.
    pub defective: bool,
    pub cost: f64,
    /// Base elapsed time in seconds.
    pub duration: f64,
    /// Whether this attempt is subject to the added queue delay.
    pub queued: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Timing {
    /// Acceptance follows the execution decision. Lateness uses base elapsed time.
    Observed,
    /// Only outputs accepted by the deadline count towards completion and defects.
    Timely,
    /// Add a deterministic or high-probability bound on total queue delay.
    /// `exceedance` bounds the probability of exceeding `max_delay` in the cell.
    QueueEnvelope { max_delay: f64, exceedance: f64 },
    /// Robust timely completion under a deterministic bound on added delay.
    TimelyQueueEnvelope { max_delay: f64 },
}

/// Aggregate jointly observed terminal outcomes, retaining rejected attempts in
/// the cost and lateness denominator. Delay must leave costs and pre-delay
/// acceptance/defect classifications valid throughout the permitted route mixes.
pub fn cell_from_outcomes(
    outcomes: &[TerminalOutcome],
    deadline: f64,
    timing: Timing,
) -> Result<Cell, Error> {
    if deadline <= 0.0 || !deadline.is_finite() {
        return Err(Error::InvalidInput(
            "deadline must be finite and positive".into(),
        ));
    }
    let (max_delay, exceedance) = match timing {
        Timing::Observed | Timing::Timely => (0.0, 0.0),
        Timing::QueueEnvelope {
            max_delay,
            exceedance,
        } => (max_delay, exceedance),
        Timing::TimelyQueueEnvelope { max_delay } => (max_delay, 0.0),
    };
    if max_delay < 0.0 || !max_delay.is_finite() || !probability(exceedance) {
        return Err(Error::InvalidInput("invalid queue delay envelope".into()));
    }
    let mut mass = 0.0;
    let mut acceptance = 0.0;
    let mut joint_defect = 0.0;
    let mut lateness = 0.0;
    let mut clean = 0.0;
    let mut cost = 0.0;
    let mut queued_mass_on_time = 0.0;
    for o in outcomes {
        let p = o.probability;
        if !probability(p)
            || o.defective && !o.accepted
            || o.duration < 0.0
            || !o.duration.is_finite()
            || o.cost < 0.0
            || !o.cost.is_finite()
        {
            return Err(Error::InvalidInput("invalid terminal outcome".into()));
        }
        mass += p;
        cost += p * o.cost;
        let actual_now = o.duration <= deadline;
        let on_time_at_bound = o.duration + if o.queued { max_delay } else { 0.0 } <= deadline;
        if !on_time_at_bound {
            lateness += p;
        } else if o.queued {
            queued_mass_on_time += p;
        }
        match timing {
            Timing::Observed | Timing::QueueEnvelope { .. } => {
                if o.accepted {
                    acceptance += p;
                }
                if o.defective {
                    joint_defect += p;
                }
            }
            Timing::Timely | Timing::TimelyQueueEnvelope { .. } => {
                if o.accepted && on_time_at_bound {
                    acceptance += p;
                }
                if o.accepted && !o.defective && on_time_at_bound {
                    clean += p;
                }
                if o.defective && actual_now {
                    joint_defect += p;
                }
            }
        }
    }
    if (mass - 1.0).abs() > 1e-9 {
        return Err(Error::InvalidInput(format!(
            "terminal probabilities sum to {mass}, expected one"
        )));
    }
    let bound = Moments {
        acceptance: (acceptance / mass).min(1.0),
        joint_defect: (joint_defect / mass).min(1.0),
        cost: cost / mass,
        lateness: (lateness / mass + exceedance.min(queued_mass_on_time / mass)).min(1.0),
    };
    let cell = match timing {
        Timing::Observed | Timing::Timely => Cell::Point(bound),
        Timing::QueueEnvelope { .. } => Cell::Conservative {
            bounds: bound,
            paired_risk: None,
        },
        Timing::TimelyQueueEnvelope { .. } => Cell::TimelyQueue {
            bounds: bound,
            clean_lower: (clean / mass).min(1.0),
        },
    };
    cell.validate()?;
    Ok(cell)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_risk_when_delay_disproportionately_removes_clean_outputs() {
        let outcomes = [
            TerminalOutcome {
                probability: 0.1,
                accepted: true,
                defective: true,
                cost: 1.0,
                duration: 8.0,
                queued: true,
            },
            TerminalOutcome {
                probability: 0.9,
                accepted: true,
                defective: false,
                cost: 1.0,
                duration: 9.5,
                queued: true,
            },
        ];
        let cell = cell_from_outcomes(
            &outcomes,
            10.,
            Timing::TimelyQueueEnvelope { max_delay: 1. },
        )
        .unwrap();
        assert_eq!(cell.moments().acceptance, 0.1);
        assert!((cell.risk(0.2).unwrap() - 0.08).abs() < 1e-12);
    }

    #[test]
    fn rejects_bad_mass_and_keeps_deadline_equality_on_time() {
        let sample = TerminalOutcome {
            probability: 1.0,
            accepted: true,
            defective: false,
            cost: 2.0,
            duration: 10.0,
            queued: false,
        };
        let cell = cell_from_outcomes(&[sample], 10.0, Timing::Timely).unwrap();
        assert_eq!(cell.moments().lateness, 0.0);
        assert_eq!(cell.moments().acceptance, 1.0);
        assert!(cell_from_outcomes(&[], 10.0, Timing::Observed).is_err());
    }
}
