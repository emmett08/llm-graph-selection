//! Compare measured deployment designs under shared load or persistent state.
//!
//! Each design has its own outcomes from complete workload runs. The comparison
//! does not assume that changing the routing weights preserves outcome means.

use std::collections::HashSet;

use crate::calibration::calibrate_mean_endpoints;
use crate::{Cell, Error, OutcomeMeans, SelectionRequirements, ServiceScope};

/// Simultaneous endpoints for five outcome means in one scenario.
///
/// Supply equal endpoints for a design with known means. When the endpoints
/// come from data, use bounds that cover all designs and scenarios together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenarioOutcomes {
    pub lower: OutcomeMeans,
    pub upper: OutcomeMeans,
}

impl ScenarioOutcomes {
    pub fn known(means: OutcomeMeans) -> Self {
        Self {
            lower: means,
            upper: means,
        }
    }

    /// Calibrate from independent repetitions of a workload of fixed size.
    ///
    /// Each observation contains the per-task mean cost and outcome proportions
    /// for one workload, including all submitted tasks. Tasks within a run may
    /// depend on one another. Set `uncertain_pairs` to the total number of
    /// uncertain design–scenario pairs in the comparison. Use the same family
    /// error probability `alpha` for each pair. The cost cap must bound the
    /// mean cost of every workload repetition in this pair.
    pub fn calibrate_workloads(
        workloads: &[OutcomeMeans],
        cost_cap: f64,
        alpha: f64,
        uncertain_pairs: usize,
    ) -> Result<Self, Error> {
        let mut mean = *workloads
            .first()
            .ok_or_else(|| Error::InvalidInput("expected at least one workload run".into()))?;
        for (index, &run) in workloads.iter().enumerate() {
            Cell::PointWithFailure(run).validate()?;
            if run.moments.cost > cost_cap {
                return Err(Error::InvalidInput(
                    "workload mean cost exceeds the supplied cost cap".into(),
                ));
            }
            let count = (index + 1) as f64;
            let update = |mean, observation| mean + (observation - mean) / count;
            mean.moments.acceptance = update(mean.moments.acceptance, run.moments.acceptance);
            mean.moments.joint_defect = update(mean.moments.joint_defect, run.moments.joint_defect);
            mean.moments.cost = update(mean.moments.cost, run.moments.cost);
            mean.moments.lateness = update(mean.moments.lateness, run.moments.lateness);
            mean.failure = update(mean.failure, run.failure);
        }
        // The running means of these two probabilities can differ by an ulp
        // even if the sample means are equal.
        mean.moments.joint_defect = mean.moments.joint_defect.min(mean.moments.acceptance);
        let n = u64::try_from(workloads.len())
            .map_err(|_| Error::InvalidInput("workload run count exceeds u64".into()))?;
        let (lower, upper) = calibrate_mean_endpoints(mean, n, cost_cap, alpha, uncertain_pairs)?;
        Ok(Self { lower, upper })
    }

    fn endpoints(self, direction: Direction) -> OutcomeMeans {
        let (mut ceilings, floors) = match direction {
            Direction::Conservative => (self.upper, self.lower),
            Direction::Optimistic => (self.lower, self.upper),
        };
        ceilings.moments.acceptance = floors.moments.acceptance;
        ceilings
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeploymentDesign {
    pub id: String,
    /// One entry per scenario in the order declared by the study.
    pub outcomes: Vec<ScenarioOutcomes>,
}

#[derive(Debug, Clone)]
pub struct DeploymentStudy {
    scenarios: Vec<String>,
    designs: Vec<DeploymentDesign>,
}

#[derive(Debug, Clone, Copy)]
pub struct DeploymentSelection<'a> {
    pub design: &'a DeploymentDesign,
    /// Largest upper bound on expected cost per submitted task.
    pub worst_cost: f64,
    /// Largest upper bound on task-failure probability.
    pub worst_failure: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct DeploymentCostBounds<'a> {
    /// A design whose conservative bounds satisfy the service limits.
    pub conservative: Option<DeploymentSelection<'a>>,
    /// Cost from the optimistic comparison, when that comparison is feasible.
    ///
    /// When all endpoints cover their means, this cost bounds the population
    /// optimum from below. An infeasible optimistic comparison rules out every
    /// design. If only the conservative comparison is infeasible, the bounds
    /// leave feasibility unresolved.
    pub optimistic_cost: Option<f64>,
}

impl DeploymentCostBounds<'_> {
    /// Upper bound on excess cost whenever all endpoints cover their means.
    pub fn cost_gap(&self) -> Option<f64> {
        Some(self.conservative?.worst_cost - self.optimistic_cost?)
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Conservative,
    Optimistic,
}

#[derive(Clone, Copy)]
enum Objective {
    Cost,
    Failure { budget: f64 },
}

impl DeploymentStudy {
    /// Compare a finite list using a common set of named scenarios.
    pub fn new(scenarios: Vec<String>, designs: Vec<DeploymentDesign>) -> Result<Self, Error> {
        if !unique_ids(scenarios.iter().map(String::as_str))
            || !unique_ids(designs.iter().map(|design| design.id.as_str()))
        {
            return Err(Error::InvalidInput(
                "deployment designs and scenarios need unique, nonempty IDs".into(),
            ));
        }
        for design in &designs {
            if design.outcomes.len() != scenarios.len() {
                return Err(Error::InvalidInput(format!(
                    "deployment design {} requires one outcome entry per scenario",
                    design.id
                )));
            }
            for (scenario, outcomes) in scenarios.iter().zip(&design.outcomes) {
                Cell::Interval {
                    lower: outcomes.lower,
                    upper: outcomes.upper,
                }
                .validate()
                .map_err(|err| Error::InvalidInput(format!("{} / {scenario}: {err}", design.id)))?;
            }
        }
        Ok(Self { scenarios, designs })
    }

    pub fn scenarios(&self) -> &[String] {
        &self.scenarios
    }

    /// Minimise worst-scenario cost subject to the scenario-wide service limits.
    /// Break cost ties by minimising worst-scenario failure.
    pub fn minimise_cost(
        &self,
        requirements: SelectionRequirements,
    ) -> Result<DeploymentCostBounds<'_>, Error> {
        validate_requirements(requirements)?;
        Ok(DeploymentCostBounds {
            conservative: self.select(requirements, Direction::Conservative, Objective::Cost),
            optimistic_cost: self
                .select(requirements, Direction::Optimistic, Objective::Cost)
                .map(|selection| selection.worst_cost),
        })
    }

    /// Minimise worst-scenario failure subject to the cost budget and service
    /// limits. Break failure ties by minimising worst-scenario cost.
    ///
    /// With confidence bounds, infeasibility means no design passes the
    /// comparison using conservative endpoints.
    pub fn minimise_failure(
        &self,
        requirements: SelectionRequirements,
        budget: f64,
    ) -> Result<DeploymentSelection<'_>, Error> {
        validate_requirements(requirements)?;
        if !budget.is_finite() || budget < 0.0 {
            return Err(Error::InvalidInput(
                "budget must be finite and nonnegative".into(),
            ));
        }
        self.select(
            requirements,
            Direction::Conservative,
            Objective::Failure { budget },
        )
        .ok_or(Error::Infeasible)
    }

    fn select(
        &self,
        requirements: SelectionRequirements,
        direction: Direction,
        objective: Objective,
    ) -> Option<DeploymentSelection<'_>> {
        self.designs
            .iter()
            .filter_map(|design| {
                let mut selection = DeploymentSelection {
                    design,
                    worst_cost: 0.0,
                    worst_failure: 0.0,
                };
                for outcomes in &design.outcomes {
                    let means = outcomes.endpoints(direction);
                    if !satisfies(means, requirements) {
                        return None;
                    }
                    selection.worst_cost = selection.worst_cost.max(means.moments.cost);
                    selection.worst_failure = selection.worst_failure.max(means.failure);
                }
                match objective {
                    Objective::Failure { budget } if selection.worst_cost > budget => None,
                    _ => Some(selection),
                }
            })
            .min_by(|left, right| {
                let objectives = |selection: &DeploymentSelection<'_>| match objective {
                    Objective::Cost => (selection.worst_cost, selection.worst_failure),
                    Objective::Failure { .. } => (selection.worst_failure, selection.worst_cost),
                };
                let (left_first, left_second) = objectives(left);
                let (right_first, right_second) = objectives(right);
                left_first
                    .total_cmp(&right_first)
                    .then_with(|| left_second.total_cmp(&right_second))
            })
    }
}

fn unique_ids<'a>(mut ids: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    let valid = ids.all(|id| !id.trim().is_empty() && seen.insert(id));
    valid && !seen.is_empty()
}

fn validate_requirements(requirements: SelectionRequirements) -> Result<(), Error> {
    requirements.validate()?;
    if requirements.scope != ServiceScope::Scenario {
        return Err(Error::InvalidInput(
            "deployment design comparisons require scenario-wide service limits".into(),
        ));
    }
    Ok(())
}

fn satisfies(means: OutcomeMeans, requirements: SelectionRequirements) -> bool {
    let moments = means.moments;
    requirements
        .max_failure
        .is_none_or(|limit| means.failure <= limit)
        && requirements
            .completion
            .is_none_or(|floor| moments.acceptance >= floor)
        && requirements
            .max_defect
            .is_none_or(|limit| limit == 1.0 || moments.joint_defect <= limit * moments.acceptance)
        && requirements
            .max_lateness
            .is_none_or(|limit| moments.lateness <= limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Moments;

    const LIMITS: SelectionRequirements = SelectionRequirements {
        max_failure: None,
        completion: None,
        max_defect: None,
        max_lateness: None,
        scope: ServiceScope::Scenario,
    };

    fn means(cost: f64, failure: f64) -> OutcomeMeans {
        OutcomeMeans {
            moments: Moments {
                acceptance: 1.0,
                joint_defect: 0.0,
                cost,
                lateness: failure,
            },
            failure,
        }
    }

    fn design(id: &str, outcomes: Vec<ScenarioOutcomes>) -> DeploymentDesign {
        DeploymentDesign {
            id: id.into(),
            outcomes,
        }
    }

    #[test]
    fn coordinated_assignment_meets_the_failure_limit_under_shared_load() {
        let arrivals = vec!["separate".into(), "simultaneous".into()];
        let independent = design(
            "independent",
            vec![
                ScenarioOutcomes::known(means(1.0, 0.0)),
                ScenarioOutcomes::known(means(1.0, 0.5)),
            ],
        );
        let coordinated = design(
            "coordinated",
            vec![ScenarioOutcomes::known(means(2.0, 0.0)); 2],
        );
        let study = DeploymentStudy::new(arrivals, vec![independent, coordinated]).unwrap();
        let result = study
            .minimise_cost(SelectionRequirements {
                max_failure: Some(0.15),
                ..LIMITS
            })
            .unwrap();
        assert_eq!(result.conservative.unwrap().design.id, "coordinated");
        assert_eq!(result.optimistic_cost, Some(2.0));
        assert_eq!(result.cost_gap(), Some(0.0));
        assert_eq!(
            study.minimise_failure(LIMITS, 1.0).unwrap().worst_failure,
            0.5
        );
        assert_eq!(
            study.minimise_failure(LIMITS, 2.0).unwrap().design.id,
            "coordinated"
        );
    }

    #[test]
    fn cost_comparison_distinguishes_both_feasibility_checks() {
        let study = DeploymentStudy::new(
            vec!["burst".into()],
            vec![
                design(
                    "cheap",
                    vec![ScenarioOutcomes {
                        lower: means(1.0, 0.1),
                        upper: means(2.0, 0.2),
                    }],
                ),
                design("reliable", vec![ScenarioOutcomes::known(means(3.0, 0.12))]),
            ],
        )
        .unwrap();
        let at_limit = |limit| {
            study
                .minimise_cost(SelectionRequirements {
                    max_failure: Some(limit),
                    ..LIMITS
                })
                .unwrap()
        };

        let certified = at_limit(0.15);
        assert_eq!(certified.conservative.unwrap().design.id, "reliable");
        assert_eq!(certified.optimistic_cost, Some(1.0));
        assert_eq!(certified.cost_gap(), Some(2.0));
        let unresolved = at_limit(0.11);
        assert!(unresolved.conservative.is_none());
        assert_eq!(unresolved.optimistic_cost, Some(1.0));
        assert!(at_limit(0.09).optimistic_cost.is_none());
    }

    #[test]
    fn bounds_use_workload_runs_and_validate_each_observed_cost() {
        let runs = vec![means(1.0, 0.2); 200];
        let bounds = ScenarioOutcomes::calibrate_workloads(&runs, 1.0, 0.05, 2).unwrap();
        let radius = ((10.0_f64 * 2.0 / 0.05).ln() / 400.0).sqrt();
        assert!((bounds.lower.failure - (0.2 - radius)).abs() < 1e-12);
        assert!((bounds.upper.failure - (0.2 + radius)).abs() < 1e-12);
        assert!(ScenarioOutcomes::calibrate_workloads(&runs, 0.9, 0.05, 2).is_err());
    }

    #[test]
    fn accepts_vacuous_defect_limit_with_separate_endpoints() {
        let mut lower = means(1.0, 0.1);
        lower.moments.acceptance = 0.5;
        let mut upper = means(2.0, 0.25);
        upper.moments.joint_defect = 0.6;
        let study = DeploymentStudy::new(
            vec!["normal".into()],
            vec![design("fixed", vec![ScenarioOutcomes { lower, upper }])],
        )
        .unwrap();
        let result = study
            .minimise_cost(SelectionRequirements {
                completion: Some(0.5),
                max_defect: Some(1.0),
                ..LIMITS
            })
            .unwrap();
        assert!(result.conservative.is_some());
    }
}
