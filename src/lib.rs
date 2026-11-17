//! Select configurations and routing rules using whole-workflow cost and failure.
//!
//! Each group uses the same policy in every scenario. Each `Cell` contains
//! whole-workflow moments or conservative coefficients for one combination
//! of group, candidate and scenario. The deployment assumptions governing
//! those coefficients are part of the study, rather than solver settings.

mod assessed;
pub mod calibration;
pub mod deployment;
pub mod graph;
pub mod io;
mod optimisation;
pub mod outcomes;
mod simplex;

pub use assessed::AssessedGroups;
pub use optimisation::{BudgetSolution, CalibrationSelection, Policy, PricedPolicy, ServiceReport};

use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    InvalidInput(String),
    Infeasible,
    Numerical(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::Infeasible => f.write_str("no solution meets the supplied constraints"),
            Self::Numerical(message) => write!(f, "numerical optimisation error: {message}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moments {
    pub acceptance: f64,
    pub joint_defect: f64,
    pub cost: f64,
    pub lateness: f64,
}

/// Cell means, including end-to-end failure among all submitted tasks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutcomeMeans {
    pub moments: Moments,
    pub failure: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiskBound {
    pub epsilon: f64,
    pub upper: f64,
}

/// Inputs to one cell. Conservative acceptance is a lower bound. Cost,
/// lateness and joint defects are upper bounds. They need not jointly
/// constitute a feasible probability distribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cell {
    Point(Moments),
    PointWithFailure(OutcomeMeans),
    Conservative {
        bounds: Moments,
        paired_risk: Option<RiskBound>,
    },
    /// Bounds for acceptance by the deadline when accumulated delay lies in
    /// a known interval. `clean_lower` is a lower bound on timely clean output.
    TimelyQueue {
        bounds: Moments,
        clean_lower: f64,
    },
    /// Upper failure bound, with an optional lower bound on clean timely output.
    FailureBound {
        bounds: Moments,
        failure_upper: f64,
        clean_lower: Option<f64>,
    },
    /// Simultaneous lower and upper endpoints. A normal solve uses the
    /// conservative combination of these endpoints.
    Interval {
        lower: OutcomeMeans,
        upper: OutcomeMeans,
    },
}

impl Cell {
    fn moments(self) -> Moments {
        match self {
            Self::Point(x)
            | Self::Conservative { bounds: x, .. }
            | Self::TimelyQueue { bounds: x, .. }
            | Self::FailureBound { bounds: x, .. } => x,
            Self::PointWithFailure(x) => x.moments,
            Self::Interval { lower, upper } => Moments {
                acceptance: lower.moments.acceptance,
                ..upper.moments
            },
        }
    }

    /// Point failure probability or its conservative upper bound, when supplied.
    pub fn failure(self) -> Option<f64> {
        match self {
            Self::PointWithFailure(x) => Some(x.failure),
            Self::Interval { upper, .. } => Some(upper.failure),
            Self::FailureBound { failure_upper, .. } => Some(failure_upper),
            Self::TimelyQueue { clean_lower, .. } => Some(1.0 - clean_lower),
            _ => None,
        }
    }

    fn is_point(self) -> bool {
        matches!(self, Self::Point(_) | Self::PointWithFailure(_))
    }

    fn uses_timely_score(self) -> bool {
        matches!(
            self,
            Self::TimelyQueue { .. }
                | Self::FailureBound {
                    clean_lower: Some(_),
                    ..
                }
        )
    }

    fn optimistic(self) -> Result<Self, Error> {
        match self {
            Self::Interval { lower, upper } => Ok(Self::FailureBound {
                bounds: Moments {
                    acceptance: upper.moments.acceptance,
                    ..lower.moments
                },
                failure_upper: lower.failure,
                clean_lower: None,
            }),
            cell if cell.is_point() => Ok(cell),
            _ => Err(Error::InvalidInput(
                "cost-gap comparison requires exact means or two-sided bounds".into(),
            )),
        }
    }

    fn risk(self, epsilon: f64) -> Result<f64, Error> {
        let m = self.moments();
        match self {
            Self::Conservative {
                paired_risk: Some(bound),
                ..
            } => {
                if bound.epsilon != epsilon {
                    return Err(Error::InvalidInput(format!(
                        "paired risk bounds require epsilon={}, received {epsilon}",
                        bound.epsilon
                    )));
                }
                Ok(bound.upper)
            }
            Self::TimelyQueue { clean_lower, .. }
            | Self::FailureBound {
                clean_lower: Some(clean_lower),
                ..
            } => Ok((1.0 - epsilon) * m.joint_defect - epsilon * clean_lower),
            _ => Ok(m.joint_defect - epsilon * m.acceptance),
        }
    }

    fn validate(self) -> Result<(), Error> {
        let m = self.moments();
        if !probability(m.acceptance)
            || !probability(m.joint_defect)
            || !probability(m.lateness)
            || !m.cost.is_finite()
            || m.cost < 0.0
        {
            return Err(Error::InvalidInput("invalid cell moments".into()));
        }
        if self.is_point() && m.joint_defect > m.acceptance {
            return Err(Error::InvalidInput(
                "joint defects exceed acceptance".into(),
            ));
        }
        if let Self::Conservative {
            paired_risk: Some(risk),
            ..
        } = self
        {
            if !probability(risk.epsilon)
                || !risk.upper.is_finite()
                || risk.upper < -risk.epsilon - 1e-12
                || risk.upper > 1.0 - risk.epsilon + 1e-12
            {
                return Err(Error::InvalidInput("invalid paired risk bound".into()));
            }
        }
        if let Self::TimelyQueue { clean_lower, .. }
        | Self::FailureBound {
            clean_lower: Some(clean_lower),
            ..
        } = self
        {
            if !probability(clean_lower) || clean_lower > m.acceptance {
                return Err(Error::InvalidInput(
                    "invalid timely clean lower bound".into(),
                ));
            }
        }
        if self.failure().is_some_and(|failure| !probability(failure)) {
            return Err(Error::InvalidInput("failure must lie in [0,1]".into()));
        }
        if let Self::Interval { lower, upper } = self {
            let a = lower.moments;
            let b = upper.moments;
            if !probability(b.acceptance)
                || !probability(a.joint_defect)
                || !probability(a.lateness)
                || !a.cost.is_finite()
                || a.cost < 0.0
                || !probability(lower.failure)
                || a.acceptance > b.acceptance
                || a.joint_defect > b.joint_defect
                || a.lateness > b.lateness
                || a.cost > b.cost
                || lower.failure > upper.failure
                || a.joint_defect > b.acceptance
            {
                return Err(Error::InvalidInput("invalid two-sided cell bounds".into()));
            }
        }
        Ok(())
    }
}

pub(crate) fn probability(value: f64) -> bool {
    (0.0..=1.0).contains(&value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceScope {
    Scenario,
    Group,
    /// Independently assessed service groups, with routing still by observed group.
    Assessed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Requirements {
    pub epsilon: f64,
    pub completion: f64,
    pub lateness: f64,
    pub scope: ServiceScope,
}

impl Requirements {
    fn validate(self) -> Result<(), Error> {
        if probability(self.epsilon)
            && probability(self.completion)
            && self.completion > 0.0
            && probability(self.lateness)
        {
            Ok(())
        } else {
            Err(Error::InvalidInput(
                "requires epsilon and lateness in [0,1] and completion in (0,1]".into(),
            ))
        }
    }
}

/// Service limits for selecting a configuration or routing rule. The failure
/// ceiling applies to every scenario and also to any selected service groups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionRequirements {
    pub max_failure: Option<f64>,
    pub completion: Option<f64>,
    pub max_defect: Option<f64>,
    pub max_lateness: Option<f64>,
    pub scope: ServiceScope,
}

impl SelectionRequirements {
    fn validate(self) -> Result<(), Error> {
        let limits = [
            self.max_failure,
            self.completion,
            self.max_defect,
            self.max_lateness,
        ];
        if !limits.into_iter().flatten().all(probability) {
            return Err(Error::InvalidInput(
                "service limits must lie in [0,1]".into(),
            ));
        }
        if self.max_defect.is_some() && !self.completion.is_some_and(|value| value > 0.0) {
            return Err(Error::InvalidInput(
                "a conditional defect limit requires a positive acceptance floor".into(),
            ));
        }
        Ok(())
    }

    fn epsilon(self) -> f64 {
        self.max_defect.unwrap_or(1.0)
    }
}

impl From<Requirements> for SelectionRequirements {
    fn from(value: Requirements) -> Self {
        Self {
            max_failure: None,
            completion: Some(value.completion),
            max_defect: Some(value.epsilon),
            max_lateness: Some(value.lateness),
            scope: value.scope,
        }
    }
}

/// Candidate sets for comparison. Family identifiers follow Study::candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingRestriction {
    Unrestricted,
    OneFamily(Vec<String>),
    OneConfiguration,
}

#[derive(Debug, Clone)]
pub struct Study {
    groups: Vec<String>,
    candidates: Vec<String>,
    scenarios: Vec<String>,
    proportions: Vec<Vec<f64>>,
    cells: Vec<Cell>,
    eligible: Vec<Vec<bool>>,
    assessed: Option<AssessedGroups>,
}

impl Study {
    /// Construct a study. Proportions are indexed by scenario then group.
    /// Cells are flat in group, candidate, scenario order (scenario varies fastest).
    pub fn new(
        groups: Vec<String>,
        candidates: Vec<String>,
        scenarios: Vec<String>,
        proportions: Vec<Vec<f64>>,
        cells: Vec<Cell>,
    ) -> Result<Self, Error> {
        for ids in [&groups, &candidates, &scenarios] {
            let unique: HashSet<&str> = ids.iter().map(String::as_str).collect();
            if ids.is_empty()
                || unique.len() != ids.len()
                || ids.iter().any(|id| id.trim().is_empty())
            {
                return Err(Error::InvalidInput(
                    "each axis needs unique, non-empty IDs".into(),
                ));
            }
        }
        let expected = groups
            .len()
            .checked_mul(candidates.len())
            .and_then(|count| count.checked_mul(scenarios.len()));
        if Some(cells.len()) != expected || proportions.len() != scenarios.len() {
            return Err(Error::InvalidInput(
                "cell or proportion dimensions do not match the axes".into(),
            ));
        }
        let mut proportions = proportions;
        for row in &mut proportions {
            let sum: f64 = row.iter().sum();
            if row.len() != groups.len()
                || !row.iter().copied().all(probability)
                || (sum - 1.0).abs() > 1e-9
            {
                return Err(Error::InvalidInput(
                    "each scenario needs group probabilities summing to one".into(),
                ));
            }
            for probability in row {
                *probability /= sum;
            }
        }
        for (index, cell) in cells.iter().enumerate() {
            cell.validate()
                .map_err(|err| Error::InvalidInput(format!("cell {index}: {err}")))?;
        }
        let eligible = vec![vec![true; candidates.len()]; groups.len()];
        Ok(Self {
            groups,
            candidates,
            scenarios,
            proportions,
            cells,
            eligible,
            assessed: None,
        })
    }

    /// Set which configurations may be chosen in each observed group.
    pub fn with_eligibility(mut self, eligible: Vec<Vec<bool>>) -> Result<Self, Error> {
        if eligible.len() != self.groups.len()
            || eligible
                .iter()
                .any(|row| row.len() != self.candidates.len() || !row.iter().any(|&value| value))
        {
            return Err(Error::InvalidInput(
                "eligibility must match the group and candidate axes, with a choice for every group".into(),
            ));
        }
        self.eligible = eligible;
        Ok(self)
    }

    /// Attach outcome coefficients for independently assessed service groups.
    pub fn with_assessed_groups(mut self, assessed: AssessedGroups) -> Result<Self, Error> {
        assessed.validate_dimensions(&self)?;
        self.assessed = Some(assessed);
        Ok(self)
    }

    pub fn assessed_groups(&self) -> Option<&[String]> {
        self.assessed.as_ref().map(AssessedGroups::labels)
    }

    pub fn groups(&self) -> &[String] {
        &self.groups
    }
    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }
    pub fn scenarios(&self) -> &[String] {
        &self.scenarios
    }
    pub fn proportions(&self) -> &[Vec<f64>] {
        &self.proportions
    }

    pub fn cell(&self, group: usize, candidate: usize, scenario: usize) -> Option<Cell> {
        if group < self.groups.len()
            && candidate < self.candidates.len()
            && scenario < self.scenarios.len()
        {
            Some(self.at(group, candidate, scenario))
        } else {
            None
        }
    }

    fn at(&self, g: usize, j: usize, k: usize) -> Cell {
        self.cells[(g * self.candidates.len() + j) * self.scenarios.len() + k]
    }
}
