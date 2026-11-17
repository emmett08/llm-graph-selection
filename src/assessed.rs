//! Outcome cells for independently assessed service groups.

use crate::{probability, Cell, Error, Study};
use std::collections::HashSet;

/// Conditional outcome coefficients for service groups assessed separately from
/// the group observed by the router.
///
/// For an assessed group `h` in scenario `k`, each supplied row gives the
/// probabilities of the observed groups conditional on `h` and `k`. The
/// probabilities must apply throughout the permitted routing choices. When they
/// are estimated, outcome bounds alone do not account for their uncertainty.
#[derive(Debug, Clone)]
pub struct AssessedGroups {
    labels: Vec<String>,
    conditional_proportions: Vec<Vec<Option<Vec<f64>>>>,
    cells: Vec<Cell>,
}

impl AssessedGroups {
    /// Construct data for independently assessed groups.
    ///
    /// Proportions are indexed by scenario, assessed group and observed group.
    /// Use `None` for an assessed group absent in a scenario. Cell order is
    /// assessed group, observed group, candidate, scenario, with scenario varying
    /// fastest. Use valid placeholder cells for combinations of zero probability.
    /// Attaching these data to a study checks the shape against its axes.
    pub fn new(
        labels: Vec<String>,
        mut conditional_proportions: Vec<Vec<Option<Vec<f64>>>>,
        cells: Vec<Cell>,
    ) -> Result<Self, Error> {
        let unique: HashSet<_> = labels.iter().collect();
        if labels.is_empty()
            || unique.len() != labels.len()
            || labels.iter().any(|label| label.trim().is_empty())
        {
            return Err(Error::InvalidInput(
                "assessed groups need unique, non-empty labels".into(),
            ));
        }
        if conditional_proportions.is_empty()
            || conditional_proportions
                .iter()
                .any(|row| row.len() != labels.len())
        {
            return Err(Error::InvalidInput(
                "each scenario needs one conditional proportion entry per assessed group".into(),
            ));
        }
        for row in conditional_proportions.iter_mut().flatten().flatten() {
            let sum: f64 = row.iter().sum();
            if !row.iter().copied().all(probability) || (sum - 1.0).abs() > 1e-9 {
                return Err(Error::InvalidInput(
                    "conditional observed-group proportions must sum to one".into(),
                ));
            }
            for proportion in row {
                *proportion /= sum;
            }
        }
        for (index, cell) in cells.iter().enumerate() {
            cell.validate()
                .map_err(|err| Error::InvalidInput(format!("assessed cell {index}: {err}")))?;
        }
        Ok(Self {
            labels,
            conditional_proportions,
            cells,
        })
    }

    /// Labels in the same order used by the input arrays.
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub(crate) fn validate_dimensions(&self, study: &Study) -> Result<(), Error> {
        let expected = self
            .labels
            .len()
            .checked_mul(study.groups().len())
            .and_then(|n| n.checked_mul(study.candidates().len()))
            .and_then(|n| n.checked_mul(study.scenarios().len()));
        if expected != Some(self.cells.len())
            || self.conditional_proportions.len() != study.scenarios().len()
        {
            return Err(Error::InvalidInput(
                "assessed cell or scenario dimensions do not match the study axes".into(),
            ));
        }
        for (k, scenario) in self.conditional_proportions.iter().enumerate() {
            for (h, conditional) in scenario.iter().enumerate() {
                let Some(conditional) = conditional else {
                    continue;
                };
                let observed = &study.proportions()[k];
                if conditional.len() != observed.len()
                    || conditional
                        .iter()
                        .zip(observed)
                        .any(|(&within_group, &overall)| within_group > 0.0 && overall == 0.0)
                {
                    return Err(Error::InvalidInput(format!(
                        "conditional proportions for assessed group {} in scenario {} do not match the observed groups and their support",
                        self.labels[h],
                        study.scenarios()[k],
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn proportions(&self, scenario: usize, assessed_group: usize) -> Option<&[f64]> {
        self.conditional_proportions[scenario][assessed_group].as_deref()
    }

    pub(crate) fn at(
        &self,
        study: &Study,
        assessed_group: usize,
        observed_group: usize,
        candidate: usize,
        scenario: usize,
    ) -> Cell {
        let index = ((assessed_group * study.groups().len() + observed_group)
            * study.candidates().len()
            + candidate)
            * study.scenarios().len()
            + scenario;
        self.cells[index]
    }

    pub(crate) fn project(
        &self,
        f: impl FnMut(Cell) -> Result<Cell, Error>,
    ) -> Result<Self, Error> {
        let cells = self
            .cells
            .iter()
            .copied()
            .map(f)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            labels: self.labels.clone(),
            conditional_proportions: self.conditional_proportions.clone(),
            cells,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Moments, OutcomeMeans, RoutingRestriction, SelectionRequirements, ServiceScope};

    fn observed_study(cell: Cell) -> Study {
        Study::new(
            vec!["routine".into(), "urgent".into()],
            vec!["first".into(), "second".into()],
            vec!["ordinary".into(), "surge".into()],
            vec![vec![0.8, 0.2], vec![1.0, 0.0]],
            vec![cell; 8],
        )
        .unwrap()
    }

    fn cell(cost: f64) -> Cell {
        Cell::Point(Moments {
            acceptance: 1.0,
            joint_defect: 0.0,
            cost,
            lateness: 0.0,
        })
    }

    #[test]
    fn indexes_scenarios_and_omits_absent_assessed_groups() {
        let study = observed_study(cell(1.0));
        let assessed = AssessedGroups::new(
            vec!["routine".into(), "urgent".into()],
            vec![
                vec![Some(vec![1.0, 0.0]), Some(vec![0.4, 0.6])],
                vec![Some(vec![1.0, 0.0]), None],
            ],
            (0..16).map(|index| cell(f64::from(index))).collect(),
        )
        .unwrap();
        assessed.validate_dimensions(&study).unwrap();
        assert_eq!(assessed.proportions(0, 1), Some([0.4, 0.6].as_slice()));
        assert_eq!(assessed.proportions(1, 1), None);
        assert_eq!(assessed.at(&study, 1, 1, 0, 0), cell(12.0));
        assert_eq!(assessed.at(&study, 1, 0, 1, 1), cell(11.0));
    }

    #[test]
    fn rejects_missing_probability_mass_and_impossible_observed_group() {
        let study = observed_study(cell(1.0));
        let labels = vec!["assessed".into()];
        let cells = vec![cell(1.0); 8];
        assert!(AssessedGroups::new(
            labels.clone(),
            vec![vec![Some(vec![0.5, 0.4])], vec![None]],
            cells.clone(),
        )
        .is_err());
        let assessed = AssessedGroups::new(
            labels,
            vec![vec![Some(vec![0.8, 0.2])], vec![Some(vec![0.8, 0.2])]],
            cells,
        )
        .unwrap();
        assert!(assessed.validate_dimensions(&study).is_err());
    }

    #[test]
    fn limits_independently_assessed_groups_using_observed_routing_weights() {
        let outcome = |cost, acceptance| {
            Cell::Point(Moments {
                acceptance,
                joint_defect: 0.0,
                cost,
                lateness: 0.0,
            })
        };
        let cheap = outcome(1.0, 1.0);
        let safe = outcome(2.0, 1.0);
        let study = Study::new(
            vec!["routine".into(), "urgent".into()],
            vec!["cheap".into(), "safe".into()],
            vec!["nominal".into()],
            vec![vec![0.9, 0.1]],
            vec![outcome(1.0, 8.0 / 9.0), safe, cheap, safe],
        )
        .unwrap();
        let assessed = AssessedGroups::new(
            vec!["routine".into(), "urgent".into()],
            vec![vec![Some(vec![1.0, 0.0]), Some(vec![0.5, 0.5])]],
            vec![
                cheap,
                safe,
                cheap,
                safe,
                outcome(1.0, 0.0),
                safe,
                cheap,
                safe,
            ],
        )
        .unwrap();
        let study = study.with_assessed_groups(assessed).unwrap();
        let req = SelectionRequirements {
            completion: Some(0.75),
            max_failure: None,
            max_defect: None,
            max_lateness: None,
            scope: ServiceScope::Group,
        };
        let routing = RoutingRestriction::Unrestricted;
        let by_observation = study.solve_selection(req, &routing, true).unwrap();
        assert!((by_observation.worst_cost - 1.0).abs() < 1e-8);
        let by_assessment = study
            .solve_selection(
                SelectionRequirements {
                    scope: ServiceScope::Assessed,
                    ..req
                },
                &routing,
                true,
            )
            .unwrap();
        // Half of urgent tasks arrive labelled routine. Meeting the assessed
        // floor requires the safe configuration on half of routine-labelled tasks.
        assert!((by_assessment.worst_cost - 1.45).abs() < 1e-8);
        assert!((by_assessment.weights[0][1] - 0.5).abs() < 1e-8);
        assert!(by_assessment.weights[1][1].abs() < 1e-8);
    }

    #[test]
    fn failure_tie_break_respects_the_independent_assessed_group_ceiling() {
        let outcome = |failure| {
            Cell::PointWithFailure(OutcomeMeans {
                moments: Moments {
                    acceptance: 1.0,
                    joint_defect: failure,
                    cost: 1.0,
                    lateness: 0.0,
                },
                failure,
            })
        };
        let clean = outcome(0.0);
        let urgent = outcome(0.3);
        let study = Study::new(
            vec!["routine".into(), "urgent".into()],
            vec!["candidate".into()],
            vec!["nominal".into()],
            vec![vec![0.9, 0.1]],
            vec![clean, urgent],
        )
        .unwrap();
        let assessed = AssessedGroups::new(
            vec!["routine".into(), "urgent".into()],
            vec![vec![Some(vec![1.0, 0.0]), Some(vec![0.0, 1.0])]],
            vec![clean, urgent, clean, urgent],
        )
        .unwrap();
        let selected = study
            .with_assessed_groups(assessed)
            .unwrap()
            .minimise_failure(
                SelectionRequirements {
                    max_failure: Some(0.4),
                    completion: None,
                    max_defect: None,
                    max_lateness: None,
                    scope: ServiceScope::Assessed,
                },
                &RoutingRestriction::Unrestricted,
                1.0,
                true,
            )
            .unwrap();
        assert!((selected.worst_failure.unwrap() - 0.03).abs() < 1e-8);
        assert!((selected.assessed[1][0].unwrap().failure.unwrap() - 0.3).abs() < 1e-8);
    }
}
