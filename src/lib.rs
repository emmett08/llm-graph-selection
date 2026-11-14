//! Optimise a routing policy over complete, evaluated LLM workflows.
//!
//! Each group uses the same policy in every scenario. Each `Cell` contains
//! whole-workflow moments or conservative coefficients for one combination
//! of group, candidate and scenario. The deployment assumptions governing
//! those coefficients are part of the study, rather than solver settings.

pub mod calibration;
pub mod graph;
pub mod io;
mod optimisation;
pub mod outcomes;
mod simplex;

pub use optimisation::{BudgetSolution, Policy, PricedPolicy, ServiceReport};

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
            Self::Infeasible => f.write_str("no routing policy meets the supplied constraints"),
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
}

impl Cell {
    fn moments(self) -> Moments {
        match self {
            Self::Point(x)
            | Self::Conservative { bounds: x, .. }
            | Self::TimelyQueue { bounds: x, .. } => x,
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
            Self::TimelyQueue { clean_lower, .. } => {
                Ok((1.0 - epsilon) * m.joint_defect - epsilon * clean_lower)
            }
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
        if matches!(self, Self::Point(_)) && m.joint_defect > m.acceptance {
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
        if let Self::TimelyQueue { clean_lower, .. } = self {
            if !probability(clean_lower) || clean_lower > m.acceptance {
                return Err(Error::InvalidInput(
                    "invalid timely clean lower bound".into(),
                ));
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

#[derive(Debug, Clone)]
pub struct Study {
    groups: Vec<String>,
    candidates: Vec<String>,
    scenarios: Vec<String>,
    proportions: Vec<Vec<f64>>,
    cells: Vec<Cell>,
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
        Ok(Self {
            groups,
            candidates,
            scenarios,
            proportions,
            cells,
        })
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

    fn check_risk(&self, epsilon: f64) -> Result<(), Error> {
        for cell in &self.cells {
            cell.risk(epsilon)?;
        }
        Ok(())
    }
}
