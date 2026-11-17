use crate::{
    simplex, Cell, Error, Requirements, RoutingRestriction, SelectionRequirements, ServiceScope,
    Study,
};
use std::collections::BTreeMap;

const CHECK: f64 = 1e-8;

#[derive(Debug, Clone, Copy, Default)]
pub struct ServiceReport {
    pub acceptance: f64,
    pub joint_defect: f64,
    pub cost: f64,
    pub lateness: f64,
    /// End-to-end failure mean or conservative upper bound, when supplied.
    pub failure: Option<f64>,
    /// Ratio of the supplied moments, if acceptance is positive.
    /// With bounds, `risk_score` is the actual service constraint.
    pub defect_rate: Option<f64>,
    /// Joint defect minus tolerance times acceptance, or its supplied upper bound.
    pub risk_score: f64,
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub worst_cost: f64,
    pub worst_failure: Option<f64>,
    /// Weights indexed by group and candidate.
    pub weights: Vec<Vec<f64>>,
    pub active_columns: usize,
    pub scenarios: Vec<ServiceReport>,
    /// Service indexed by group and scenario.
    pub groups: Vec<Vec<ServiceReport>>,
    /// Service reports indexed by assessed group and scenario.
    pub assessed: Vec<Vec<Option<ServiceReport>>>,
}

impl Policy {
    /// Convert a uniform draw in `[0, 1)` into the selected candidate index.
    /// Use a fresh draw for each attempted task, conditional on its group.
    pub fn select(&self, group: usize, draw: f64) -> Result<usize, Error> {
        let row = self
            .weights
            .get(group)
            .ok_or_else(|| Error::InvalidInput("unknown routing group".into()))?;
        if !(0.0..1.0).contains(&draw)
            || !row.iter().copied().all(crate::probability)
            || (row.iter().sum::<f64>() - 1.0).abs() > 1e-9
        {
            return Err(Error::InvalidInput(
                "selection requires a valid policy row and a draw in [0, 1)".into(),
            ));
        }
        let mut cumulative = 0.0;
        let mut last = None;
        for (candidate, &weight) in row.iter().enumerate() {
            if weight > 0.0 {
                cumulative += weight;
                last = Some(candidate);
                if draw < cumulative {
                    return Ok(candidate);
                }
            }
        }
        last.ok_or_else(|| Error::Numerical("empty routing distribution".into()))
    }
}

#[derive(Debug, Clone)]
pub struct BudgetSolution {
    pub lower: f64,
    pub upper: f64,
    pub policy: Policy,
}

#[derive(Debug, Clone)]
pub struct PricedPolicy {
    pub policy: Policy,
    pub iterations: usize,
    /// Upper bound on the cost gap over the supplied finite pool, to LP precision.
    pub gap_bound: f64,
    pub retained: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct CalibrationSelection {
    /// A solution that meets the requirements using the conservative endpoints.
    pub conservative: Option<Policy>,
    /// Lower bound on the population minimum, if the optimistic comparison is feasible.
    pub optimistic_cost: Option<f64>,
    /// Upper bound on the selected rule's excess cost on simultaneous coverage.
    pub cost_gap: Option<f64>,
}

#[derive(Clone, Copy)]
enum Population {
    Scenario,
    Observed(usize),
    Assessed(usize),
}

impl Population {
    fn coefficient_cell(self, study: &Study, g: usize, j: usize, k: usize) -> Option<(f64, Cell)> {
        let (weight, cell) = match self {
            Self::Scenario => (study.proportions[k][g], study.at(g, j, k)),
            Self::Observed(group) => (f64::from(g == group), study.at(g, j, k)),
            Self::Assessed(group) => {
                let assessed = study.assessed.as_ref()?;
                let shares = assessed.proportions(k, group)?;
                (shares[g], assessed.at(study, group, g, j, k))
            }
        };
        (weight > 0.0).then_some((weight, cell))
    }
}

#[derive(Clone, Copy)]
enum Measure {
    Failure,
    Risk,
    Completion,
    Late,
}

impl Measure {
    fn coefficient(self, cell: Cell, epsilon: f64) -> f64 {
        let m = cell.moments();
        match self {
            Self::Failure => cell
                .failure()
                .expect("failure data validated before solving"),
            Self::Risk => risk(cell, epsilon),
            Self::Completion => -m.acceptance,
            Self::Late => m.lateness,
        }
    }
}

#[derive(Clone, Copy)]
enum Objective {
    Cost,
    CostAtFailure { cap: f64 },
    Failure { budget: f64 },
}

#[derive(Clone, Copy)]
enum Row {
    Cost(usize, Option<f64>),
    Limit(usize, Population, Measure, f64),
    FailureEpigraph(usize),
    GroupSum(usize, f64),
    Budget(f64),
}

struct Master {
    columns: Vec<(usize, usize)>,
    rows: Vec<Row>,
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    cost_unit: f64,
    objective: Objective,
}

impl Row {
    fn coefficient(self, study: &Study, g: usize, j: usize, epsilon: f64, unit: f64) -> f64 {
        match self {
            Self::Cost(k, _) => study.proportions[k][g] * (study.at(g, j, k).moments().cost / unit),
            Self::Limit(k, population, measure, _) => population
                .coefficient_cell(study, g, j, k)
                .map_or(0.0, |(w, cell)| w * measure.coefficient(cell, epsilon)),
            Self::FailureEpigraph(k) => Population::Scenario
                .coefficient_cell(study, g, j, k)
                .map_or(0.0, |(w, cell)| {
                    w * Measure::Failure.coefficient(cell, epsilon)
                }),
            Self::GroupSum(group, sign) => {
                if g == group {
                    sign
                } else {
                    0.0
                }
            }
            Self::Budget(_) => 0.0,
        }
    }

    fn epigraph(self) -> f64 {
        match self {
            Self::Cost(_, None) | Self::FailureEpigraph(_) => -1.0,
            Self::Budget(_) => 1.0,
            _ => 0.0,
        }
    }

    fn rhs(self, unit: f64) -> f64 {
        match self {
            Self::Cost(_, Some(budget)) => budget / unit,
            Self::Limit(_, _, _, limit) => limit,
            Self::GroupSum(_, sign) => sign,
            Self::Budget(budget) => budget / unit,
            _ => 0.0,
        }
    }
}

/// The constraint at epsilon=1 follows directly from Y <= A, even with bounds.
fn risk(cell: Cell, epsilon: f64) -> f64 {
    if epsilon == 1.0 && !cell.is_point() {
        return 0.0;
    }
    cell.risk(epsilon)
        .expect("risk tolerance validated before constructing LP")
}

impl Master {
    fn new(
        study: &Study,
        req: SelectionRequirements,
        choices: &[Vec<usize>],
        budget: Option<f64>,
    ) -> Self {
        Self::with_objective(study, req, choices, budget, Objective::Cost)
    }

    fn with_objective(
        study: &Study,
        req: SelectionRequirements,
        choices: &[Vec<usize>],
        budget: Option<f64>,
        objective: Objective,
    ) -> Self {
        let columns: Vec<_> = choices
            .iter()
            .enumerate()
            .flat_map(|(g, candidates)| candidates.iter().map(move |&j| (g, j)))
            .collect();
        let cost_cap = match objective {
            Objective::Cost | Objective::CostAtFailure { .. } => None,
            Objective::Failure { budget } => Some(budget),
        };
        let cost_max = study
            .cells
            .iter()
            .map(|cell| cell.moments().cost)
            .fold(cost_cap.or(budget).unwrap_or(0.0), f64::max);
        let cost_unit = if cost_max > 0.0 { cost_max } else { 1.0 };
        let rows = Self::rows(study, req, objective, budget);
        let a = rows
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|&(g, j)| row.coefficient(study, g, j, req.epsilon(), cost_unit))
                    .chain(std::iter::once(row.epigraph()))
                    .collect()
            })
            .collect();
        let b = rows.iter().map(|row| row.rhs(cost_unit)).collect();
        Self {
            columns,
            rows,
            a,
            b,
            cost_unit,
            objective,
        }
    }

    fn rows(
        study: &Study,
        req: SelectionRequirements,
        objective: Objective,
        budget: Option<f64>,
    ) -> Vec<Row> {
        let mut rows = Vec::new();
        for k in 0..study.scenarios.len() {
            let cost_cap = match objective {
                Objective::Failure { budget } => Some(budget),
                _ => None,
            };
            rows.push(Row::Cost(k, cost_cap));
            let scenario_failure_cap = match objective {
                Objective::CostAtFailure { cap } => {
                    Some(req.max_failure.map_or(cap, |x| x.min(cap)))
                }
                Objective::Failure { .. } => {
                    rows.push(Row::FailureEpigraph(k));
                    req.max_failure
                }
                Objective::Cost => req.max_failure,
            };
            if let Some(cap) = scenario_failure_cap {
                rows.push(Row::Limit(k, Population::Scenario, Measure::Failure, cap));
            }
            for population in study.service_populations(k, req.scope) {
                let mut add = |measure, limit| rows.push(Row::Limit(k, population, measure, limit));
                if req.epsilon() < 1.0 {
                    add(Measure::Risk, 0.0);
                }
                if let Some(floor) = req.completion {
                    add(Measure::Completion, -floor);
                }
                if let Some(cap) = req.max_lateness {
                    add(Measure::Late, cap);
                }
                if let Some(cap) = req
                    .max_failure
                    .filter(|_| !matches!(population, Population::Scenario))
                {
                    add(Measure::Failure, cap);
                }
            }
        }
        for g in 0..study.groups.len() {
            rows.push(Row::GroupSum(g, 1.0));
            rows.push(Row::GroupSum(g, -1.0));
        }
        if let Some(budget) = budget {
            rows.push(Row::Budget(budget));
        }
        rows
    }

    fn solve(
        &self,
        study: &Study,
        req: SelectionRequirements,
    ) -> Result<(Policy, Vec<f64>), Error> {
        let mut c = vec![0.0; self.columns.len() + 1];
        c[self.columns.len()] = -1.0;
        let solution = solve_lp(&self.a, &self.b, &c)?;
        let weights = self.weights(study, &solution)?;
        let policy = study.report(weights, req, self.columns.len())?;
        let recovered = match self.objective {
            Objective::Cost => policy.worst_cost / self.cost_unit,
            Objective::CostAtFailure { cap } => {
                if policy
                    .worst_failure
                    .is_none_or(|failure| failure > cap + CHECK)
                {
                    return Err(Error::Numerical(
                        "recovered routing rule exceeds the failure optimum".into(),
                    ));
                }
                policy.worst_cost / self.cost_unit
            }
            Objective::Failure { budget } => {
                if (policy.worst_cost - budget) / self.cost_unit > CHECK {
                    return Err(Error::Numerical(
                        "recovered routing rule exceeds budget".into(),
                    ));
                }
                policy
                    .worst_failure
                    .expect("failure data validated before solving")
            }
        };
        if (recovered - solution[self.columns.len()]).abs() > CHECK {
            return Err(Error::Numerical(
                "epigraph and recovered objective disagree".into(),
            ));
        }
        Ok((policy, solution))
    }

    fn weights(&self, study: &Study, values: &[f64]) -> Result<Vec<Vec<f64>>, Error> {
        let mut weights = vec![vec![0.0; study.candidates.len()]; study.groups.len()];
        for (&(g, j), &value) in self.columns.iter().zip(values) {
            weights[g][j] = value;
        }
        for row in &mut weights {
            let sum: f64 = row.iter().sum();
            if (sum - 1.0).abs() > CHECK {
                return Err(Error::Numerical("routing weights do not sum to one".into()));
            }
            for weight in row {
                *weight /= sum;
            }
        }
        Ok(weights)
    }

    fn dual(&self) -> Result<Vec<f64>, Error> {
        let n = self.columns.len() + 1;
        let mut objective = vec![0.0; n];
        objective[n - 1] = 1.0;
        solve_dual(&self.a, &self.b, &objective)
    }
}

fn solve_dual(a: &[Vec<f64>], b: &[f64], objective: &[f64]) -> Result<Vec<f64>, Error> {
    // For min c^T x with Ax <= b, x >= 0, maximise the lower bound
    // -b^T y subject to -A^T y <= c and y >= 0.
    let transpose = (0..objective.len())
        .map(|j| a.iter().map(|row| -row[j]).collect())
        .collect::<Vec<_>>();
    let dual_objective: Vec<_> = b.iter().map(|bound| -bound).collect();
    solve_lp(&transpose, objective, &dual_objective)
}

fn solve_lp(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> Result<Vec<f64>, Error> {
    simplex::maximise(a, b, c).map_err(|status| match status {
        simplex::Status::Infeasible => Error::Infeasible,
        simplex::Status::Unbounded => Error::Numerical("unexpected unbounded LP".into()),
        simplex::Status::Numerical => {
            Error::Numerical("LP lost numerical accuracy or exceeded its pivot budget".into())
        }
    })
}

fn feasible(result: Result<Policy, Error>) -> Result<Option<Policy>, Error> {
    match result {
        Ok(policy) => Ok(Some(policy)),
        Err(Error::Infeasible) => Ok(None),
        Err(error) => Err(error),
    }
}

fn better(candidate: &Policy, incumbent: &Policy, objective: Objective) -> bool {
    let comparison = |a: f64, b: f64| {
        if (a - b).abs() <= 32.0 * f64::EPSILON * a.max(b) {
            std::cmp::Ordering::Equal
        } else {
            a.total_cmp(&b)
        }
    };
    let cost = comparison(candidate.worst_cost, incumbent.worst_cost);
    let failure = match (candidate.worst_failure, incumbent.worst_failure) {
        (Some(a), Some(b)) => comparison(a, b),
        _ => std::cmp::Ordering::Equal,
    };
    match objective {
        Objective::Failure { .. } => failure.then(cost).is_lt(),
        _ => cost.then(failure).is_lt(),
    }
}

impl Study {
    fn service_populations(&self, k: usize, scope: ServiceScope) -> Vec<Population> {
        match scope {
            ServiceScope::Scenario => vec![Population::Scenario],
            ServiceScope::Group => self.proportions[k]
                .iter()
                .enumerate()
                .filter(|(_, weight)| **weight > 0.0)
                .map(|(g, _)| Population::Observed(g))
                .collect(),
            ServiceScope::Assessed => self
                .assessed
                .iter()
                .flat_map(|assessed| {
                    (0..assessed.labels().len()).filter(|&h| assessed.proportions(k, h).is_some())
                })
                .map(Population::Assessed)
                .collect(),
        }
    }

    fn all_choices(&self) -> Vec<Vec<usize>> {
        (0..self.groups.len())
            .map(|g| {
                (0..self.candidates.len())
                    .filter(|&j| self.eligible[g][j])
                    .collect()
            })
            .collect()
    }

    fn validate_selection(
        &self,
        req: SelectionRequirements,
        choices: &[Vec<usize>],
        needs_failure: bool,
    ) -> Result<bool, Error> {
        req.validate()?;
        if req.scope == ServiceScope::Assessed && self.assessed.is_none() {
            return Err(Error::InvalidInput(
                "assessed service scope requires independently assessed group data".into(),
            ));
        }
        let mut has_failure = true;
        for k in 0..self.scenarios.len() {
            let populations = self.service_populations(k, req.scope);
            for (g, candidates) in choices.iter().enumerate() {
                for &j in candidates {
                    if let Some((_, cell)) = Population::Scenario.coefficient_cell(self, g, j, k) {
                        has_failure &= cell.failure().is_some();
                    }
                    for &population in &populations {
                        if let Some((_, cell)) = population.coefficient_cell(self, g, j, k) {
                            if let Some(epsilon) = req.max_defect {
                                cell.risk(epsilon)?;
                            }
                            if req.max_failure.is_some() {
                                has_failure &= cell.failure().is_some();
                            }
                        }
                    }
                }
            }
        }
        if (needs_failure || req.max_failure.is_some()) && !has_failure {
            return Err(Error::InvalidInput(
                "failure selection requires failure data for every applicable cell".into(),
            ));
        }
        Ok(has_failure)
    }

    fn prune_choices(
        &self,
        req: SelectionRequirements,
        choices: &mut [Vec<usize>],
        compare_failure: bool,
    ) {
        let mut rows = Master::rows(self, req, Objective::Cost, None);
        if compare_failure && req.max_failure.is_none() {
            rows.extend((0..self.scenarios.len()).map(Row::FailureEpigraph));
        }
        for (g, candidates) in choices.iter_mut().enumerate() {
            let columns: Vec<Vec<f64>> = candidates
                .iter()
                .map(|&j| {
                    rows.iter()
                        .map(|row| row.coefficient(self, g, j, req.epsilon(), 1.0))
                        .collect()
                })
                .collect();
            let mut index = 0;
            candidates.retain(|_| {
                let current = index;
                index += 1;
                !columns.iter().enumerate().any(|(other, values)| {
                    other != current
                        && values.iter().zip(&columns[current]).all(|(a, b)| a <= b)
                        && (other < current || values != &columns[current])
                })
            });
        }
    }

    /// Minimise worst-scenario mean cost at a fixed defect tolerance.
    pub fn solve(&self, req: Requirements, prune: bool) -> Result<Policy, Error> {
        req.validate()?;
        let req = req.into();
        let mut choices = self.all_choices();
        self.validate_selection(req, &choices, false)?;
        if prune {
            self.prune_choices(req, &mut choices, false);
        }
        self.solve_choices(req, &choices, Objective::Cost, None)
    }

    fn solve_choices(
        &self,
        req: SelectionRequirements,
        choices: &[Vec<usize>],
        objective: Objective,
        budget: Option<f64>,
    ) -> Result<Policy, Error> {
        Ok(
            Master::with_objective(self, req, choices, budget, objective)
                .solve(self, req)?
                .0,
        )
    }

    /// Minimise worst-scenario expected cost. With failure data, break cost
    /// ties by minimising worst-scenario failure.
    pub fn solve_selection(
        &self,
        req: SelectionRequirements,
        restriction: &RoutingRestriction,
        prune: bool,
    ) -> Result<Policy, Error> {
        self.compare_choices(req, restriction, prune, Objective::Cost, true)
    }

    /// Minimise worst-scenario failure within a per-scenario cost budget.
    /// Break failure ties by minimising worst-scenario cost.
    pub fn minimise_failure(
        &self,
        req: SelectionRequirements,
        restriction: &RoutingRestriction,
        budget: f64,
        prune: bool,
    ) -> Result<Policy, Error> {
        if !budget.is_finite() || budget < 0.0 {
            return Err(Error::InvalidInput(
                "budget must be finite and nonnegative".into(),
            ));
        }
        self.compare_choices(req, restriction, prune, Objective::Failure { budget }, true)
    }

    fn compare_choices(
        &self,
        req: SelectionRequirements,
        restriction: &RoutingRestriction,
        prune: bool,
        objective: Objective,
        tie_break: bool,
    ) -> Result<Policy, Error> {
        let all = self.all_choices();
        let failure_objective = matches!(objective, Objective::Failure { .. });
        let has_failure = self.validate_selection(req, &all, failure_objective)?;
        let sets: Vec<Vec<Vec<usize>>> = match restriction {
            RoutingRestriction::Unrestricted => vec![all],
            RoutingRestriction::OneFamily(families) => {
                if families.len() != self.candidates.len()
                    || families.iter().any(|id| id.trim().is_empty())
                {
                    return Err(Error::InvalidInput(
                        "supply a nonempty family identifier for every candidate".into(),
                    ));
                }
                let mut grouped = BTreeMap::<&str, Vec<Vec<usize>>>::new();
                for (g, candidates) in all.iter().enumerate() {
                    for &j in candidates {
                        grouped
                            .entry(&families[j])
                            .or_insert_with(|| vec![Vec::new(); self.groups.len()])[g]
                            .push(j);
                    }
                }
                grouped.into_values().collect()
            }
            RoutingRestriction::OneConfiguration => (0..self.candidates.len())
                .filter(|&j| self.eligible.iter().all(|row| row[j]))
                .map(|j| vec![vec![j]; self.groups.len()])
                .collect(),
        };
        let mut best: Option<Policy> = None;
        for mut choices in sets {
            if choices.iter().any(Vec::is_empty) {
                continue;
            }
            if prune {
                self.prune_choices(
                    req,
                    &mut choices,
                    tie_break && has_failure || failure_objective,
                );
            }
            let primary = match self.solve_choices(req, &choices, objective, None) {
                Ok(result) => result,
                Err(Error::Infeasible) => continue,
                Err(err) => return Err(err),
            };
            let result = match objective {
                Objective::Cost if tie_break && has_failure => self.solve_choices(
                    req,
                    &choices,
                    Objective::Failure {
                        budget: primary.worst_cost,
                    },
                    None,
                )?,
                Objective::Failure { budget } if tie_break => self.solve_choices(
                    req,
                    &choices,
                    Objective::CostAtFailure {
                        cap: primary.worst_failure.unwrap(),
                    },
                    Some(budget),
                )?,
                _ => primary,
            };
            if best
                .as_ref()
                .is_none_or(|previous| better(&result, previous, objective))
            {
                best = Some(result);
            }
        }
        best.ok_or(Error::Infeasible)
    }

    /// Solve the same candidate class with conservative and optimistic cell
    /// coefficients. The reported gap is valid on simultaneous coverage of
    /// the supplied intervals and under the study's routing assumptions.
    pub fn calibrate_selection(
        &self,
        req: SelectionRequirements,
        restriction: &RoutingRestriction,
        prune: bool,
    ) -> Result<CalibrationSelection, Error> {
        let mut optimistic = self.clone();
        optimistic.cells = self
            .cells
            .iter()
            .map(|cell| cell.optimistic())
            .collect::<Result<_, _>>()?;
        if req.scope == ServiceScope::Assessed {
            optimistic.assessed = self
                .assessed
                .as_ref()
                .map(|data| data.project(Cell::optimistic))
                .transpose()?;
        }
        let conservative = feasible(self.solve_selection(req, restriction, prune))?;
        let optimistic_cost =
            feasible(optimistic.compare_choices(req, restriction, prune, Objective::Cost, false))?
                .map(|result| result.worst_cost);
        let cost_gap = match (&conservative, optimistic_cost) {
            (Some(selected), Some(lower)) => {
                if lower - selected.worst_cost > CHECK * lower.max(selected.worst_cost) {
                    return Err(Error::Numerical(
                        "optimistic minimum exceeds the conservative cost".into(),
                    ));
                }
                Some((selected.worst_cost - lower).max(0.0))
            }
            (Some(_), None) => {
                return Err(Error::Numerical(
                    "optimistic programme rejected a conservative solution".into(),
                ))
            }
            (None, _) => None,
        };
        Ok(CalibrationSelection {
            conservative,
            optimistic_cost,
            cost_gap,
        })
    }

    /// Evaluate a routing rule without imposing the service thresholds.
    pub fn evaluate_policy(&self, weights: &[Vec<f64>], epsilon: f64) -> Result<Policy, Error> {
        if !crate::probability(epsilon) || weights.len() != self.groups.len() {
            return Err(Error::InvalidInput(
                "invalid tolerance or routing weight dimensions".into(),
            ));
        }
        let mut choices = Vec::with_capacity(weights.len());
        for (g, row) in weights.iter().enumerate() {
            if row.len() != self.candidates.len()
                || row.iter().any(|&w| !crate::probability(w))
                || (row.iter().sum::<f64>() - 1.0).abs() > 1e-9
            {
                return Err(Error::InvalidInput(
                    "each routing weight row must be a probability distribution".into(),
                ));
            }
            let active: Vec<_> = row
                .iter()
                .enumerate()
                .filter(|(_, w)| **w > 0.0)
                .map(|(j, _)| j)
                .collect();
            if active.iter().any(|&j| !self.eligible[g][j]) {
                return Err(Error::InvalidInput(
                    "routing rule assigns weight to an ineligible candidate".into(),
                ));
            }
            choices.push(active);
        }
        let req = Requirements {
            epsilon,
            completion: f64::MIN_POSITIVE,
            lateness: 1.0,
            scope: ServiceScope::Scenario,
        }
        .into();
        self.validate_selection(req, &choices, false)?;
        Ok(self.summarise(
            weights.to_vec(),
            epsilon,
            self.groups.len() * self.candidates.len(),
        ))
    }

    fn summarise(&self, weights: Vec<Vec<f64>>, epsilon: f64, active_columns: usize) -> Policy {
        let rows = |population: Population, k| {
            weights.iter().enumerate().flat_map(move |(g, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(_, weight)| **weight > 0.0)
                    .filter_map(move |(j, &weight)| {
                        population
                            .coefficient_cell(self, g, j, k)
                            .map(|(share, cell)| (share * weight, cell))
                    })
            })
        };
        let scenario_reports = (0..self.scenarios.len())
            .map(|k| ServiceReport::from_cells(rows(Population::Scenario, k), epsilon))
            .collect::<Vec<_>>();
        let groups = (0..self.groups.len())
            .map(|g| {
                (0..self.scenarios.len())
                    .map(|k| ServiceReport::from_cells(rows(Population::Observed(g), k), epsilon))
                    .collect()
            })
            .collect();
        let assessed = self.assessed.as_ref().map_or_else(Vec::new, |groups| {
            (0..groups.labels().len())
                .map(|h| {
                    (0..self.scenarios.len())
                        .map(|k| {
                            groups.proportions(k, h).map(|_| {
                                ServiceReport::from_cells(rows(Population::Assessed(h), k), epsilon)
                            })
                        })
                        .collect()
                })
                .collect()
        });
        let worst_failure = scenario_reports.iter().try_fold(0.0_f64, |value, report| {
            report.failure.map(|failure| value.max(failure))
        });
        Policy {
            worst_cost: scenario_reports.iter().map(|s| s.cost).fold(0.0, f64::max),
            worst_failure,
            weights,
            active_columns,
            scenarios: scenario_reports,
            groups,
            assessed,
        }
    }

    fn report(
        &self,
        weights: Vec<Vec<f64>>,
        req: SelectionRequirements,
        active_columns: usize,
    ) -> Result<Policy, Error> {
        let policy = self.summarise(weights, req.epsilon(), active_columns);
        let failure_exceeds = |r: &ServiceReport| {
            req.max_failure
                .is_some_and(|cap| r.failure.is_none_or(|failure| failure > cap + CHECK))
        };
        let violated = |r: &ServiceReport| {
            req.completion
                .is_some_and(|floor| r.acceptance + CHECK < floor)
                || req.max_lateness.is_some_and(|cap| r.lateness > cap + CHECK)
                || req
                    .max_defect
                    .is_some_and(|_| r.risk_score > CHECK || r.acceptance <= 0.0)
                || failure_exceeds(r)
        };
        let scope_failed = match req.scope {
            ServiceScope::Scenario => policy.scenarios.iter().any(violated),
            ServiceScope::Group => policy.groups.iter().enumerate().any(|(g, reports)| {
                reports
                    .iter()
                    .enumerate()
                    .any(|(k, r)| self.proportions[k][g] > 0.0 && violated(r))
            }),
            ServiceScope::Assessed => policy.assessed.iter().flatten().flatten().any(violated),
        };
        if scope_failed
            || policy.scenarios.iter().any(failure_exceeds)
            || !policy.worst_cost.is_finite()
        {
            return Err(Error::Numerical(
                "recovered routing rule violates service or cost bounds".into(),
            ));
        }
        Ok(policy)
    }

    fn validate_budget(&self, req: Requirements, budget: f64) -> Result<(), Error> {
        req.validate()?;
        if !budget.is_finite() || budget < 0.0 {
            return Err(Error::InvalidInput(
                "budget must be finite and nonnegative".into(),
            ));
        }
        if self.cells.iter().any(|cell| {
            matches!(
                cell,
                Cell::Conservative {
                    paired_risk: Some(_),
                    ..
                }
            )
        }) {
            return Err(Error::InvalidInput(
                "budget risk search requires coefficients valid across tolerances".into(),
            ));
        }
        Ok(())
    }

    /// Minimise the worst applicable defect fraction by LP feasibility bisection.
    /// The bracket is computed in floating point and is subject to LP residuals.
    pub fn minimise_risk(
        &self,
        req: Requirements,
        budget: f64,
        tolerance: f64,
    ) -> Result<BudgetSolution, Error> {
        self.validate_budget(req, budget)?;
        if !tolerance.is_finite() || !(1e-8..1.0).contains(&tolerance) {
            return Err(Error::InvalidInput(
                "bisection tolerance must be in [1e-8,1)".into(),
            ));
        }
        let at = |epsilon| {
            let r = Requirements { epsilon, ..req };
            let choices = self.all_choices();
            self.validate_selection(r.into(), &choices, false)?;
            let r = r.into();
            let master = Master::new(self, r, &choices, Some(budget));
            let (policy, _) = master.solve(self, r)?;
            if policy.worst_cost / master.cost_unit - budget / master.cost_unit > CHECK {
                return Err(Error::Numerical("recovered policy violates budget".into()));
            }
            Ok(policy)
        };
        let mut policy = at(1.0)?;
        match at(0.0) {
            Ok(policy) => {
                return Ok(BudgetSolution {
                    lower: 0.0,
                    upper: 0.0,
                    policy,
                })
            }
            Err(Error::Infeasible) => (),
            Err(err) => return Err(err),
        }
        let (mut lower, mut upper) = (0.0, 1.0);
        while upper - lower > tolerance {
            let middle = (lower + upper) / 2.0;
            match at(middle) {
                Ok(candidate) => {
                    upper = middle;
                    policy = candidate;
                }
                Err(Error::Infeasible) => lower = middle,
                Err(err) => return Err(err),
            }
        }
        Ok(BudgetSolution {
            lower,
            upper,
            policy,
        })
    }

    /// Solve the single-scenario fixed-budget problem by Charnes–Cooper.
    /// Uses point moments or separate bounds; timely queue risk has a different denominator.
    pub fn minimise_risk_one_lp(&self, req: Requirements, budget: f64) -> Result<Policy, Error> {
        self.validate_budget(req, budget)?;
        if self.scenarios.len() != 1
            || req.scope != ServiceScope::Scenario
            || self.cells.iter().any(|&cell| cell.uses_timely_score())
        {
            return Err(Error::InvalidInput("one-LP risk minimisation requires one scenario, scenario service and point or separate endpoint moments".into()));
        }
        let master = Master::new(
            self,
            Requirements {
                epsilon: 1.0,
                ..req
            }
            .into(),
            &self.all_choices(),
            Some(budget),
        );
        let cols = &master.columns;
        let n = cols.len();
        let moment = |kept: fn(crate::Moments) -> f64| -> Vec<f64> {
            cols.iter()
                .map(|&(g, j)| self.proportions[0][g] * kept(self.at(g, j, 0).moments()))
                .collect()
        };
        let acceptance = moment(|m| m.acceptance);
        let defects = moment(|m| m.joint_defect);
        let cost = moment(|m| m.cost);
        let late = moment(|m| m.lateness);
        let row = |values: Vec<f64>, t: f64| {
            values
                .into_iter()
                .chain(std::iter::once(t))
                .collect::<Vec<_>>()
        };
        let scaled_cost = cost.iter().map(|c| c / master.cost_unit).collect();
        let mut a = vec![
            row(scaled_cost, -budget / master.cost_unit),
            row(late, -req.lateness),
            row(acceptance.iter().map(|a| -a).collect(), req.completion),
            row(acceptance.clone(), 0.0),
            row(acceptance.iter().map(|a| -a).collect(), 0.0),
        ];
        let mut b = vec![0.0, 0.0, 0.0, 1.0, -1.0];
        for g in 0..self.groups.len() {
            let sum: Vec<_> = cols
                .iter()
                .map(|&(group, _)| f64::from(group == g))
                .collect();
            a.push(row(sum.clone(), -1.0));
            a.push(row(sum.iter().map(|v| -v).collect(), 1.0));
            b.extend([0.0, 0.0]);
        }
        let objective = row(defects.iter().map(|b| -b).collect(), 0.0);
        let solution = solve_lp(&a, &b, &objective)?;
        let t = solution[n];
        if t <= 0.0 {
            return Err(Error::Numerical("invalid Charnes–Cooper scale".into()));
        }
        let values: Vec<_> = solution[..n].iter().map(|x| x / t).collect();
        let weights = master.weights(self, &values)?;
        let policy = self.report(
            weights,
            Requirements {
                epsilon: 1.0,
                ..req
            }
            .into(),
            n,
        )?;
        let ratio = policy.scenarios[0]
            .defect_rate
            .ok_or_else(|| Error::Numerical("zero fractional denominator".into()))?;
        if policy.worst_cost / master.cost_unit - budget / master.cost_unit > CHECK {
            return Err(Error::Numerical(
                "recovered fractional policy violates budget".into(),
            ));
        }
        if (ratio
            - (-objective
                .iter()
                .zip(&solution)
                .map(|(u, x)| u * x)
                .sum::<f64>()))
        .abs()
            > CHECK
        {
            return Err(Error::Numerical(
                "fractional objective and recovered policy disagree".into(),
            ));
        }
        self.report(
            policy.weights,
            Requirements {
                epsilon: ratio.min(1.0),
                ..req
            }
            .into(),
            n,
        )
    }

    /// Price every omitted candidate in the supplied finite pool. The seed must
    /// already admit a feasible policy. Each addition is available to all groups.
    pub fn column_generate(
        &self,
        req: Requirements,
        seed: &[usize],
        tolerance: f64,
    ) -> Result<PricedPolicy, Error> {
        req.validate()?;
        self.price(req.into(), seed, tolerance, false)
    }

    /// Price the finite eligible pool with the selected service constraints.
    /// An infeasible starting pool is expanded using a service-slack
    /// minimisation programme and its own dual before pricing expected cost.
    pub fn column_generate_selection(
        &self,
        req: SelectionRequirements,
        seed: &[usize],
        tolerance: f64,
    ) -> Result<PricedPolicy, Error> {
        self.price(req, seed, tolerance, true)
    }

    fn price(
        &self,
        req: SelectionRequirements,
        seed: &[usize],
        tolerance: f64,
        phase_one: bool,
    ) -> Result<PricedPolicy, Error> {
        let all = self.all_choices();
        self.validate_selection(req, &all, false)?;
        if seed.is_empty()
            || seed.iter().any(|&j| j >= self.candidates.len())
            || tolerance < 0.0
            || !tolerance.is_finite()
        {
            return Err(Error::InvalidInput(
                "invalid candidate seed or pricing tolerance".into(),
            ));
        }
        let mut retained = seed.to_vec();
        retained.sort_unstable();
        retained.dedup();
        if phase_one {
            for eligible in &all {
                if !eligible.iter().any(|j| retained.binary_search(j).is_ok()) {
                    let position = retained
                        .binary_search(&eligible[0])
                        .expect_err("omitted candidate");
                    retained.insert(position, eligible[0]);
                }
            }
        }
        for iterations in 1..=self.candidates.len() {
            let choices = all
                .iter()
                .map(|eligible| {
                    eligible
                        .iter()
                        .copied()
                        .filter(|j| retained.binary_search(j).is_ok())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let master = Master::new(self, req, &choices, None);
            let policy = match master.solve(self, req) {
                Ok((policy, _)) => policy,
                Err(Error::Infeasible) if phase_one => {
                    let candidate = self.price_feasibility(req, &master, &all, &retained)?;
                    let position = retained
                        .binary_search(&candidate)
                        .expect_err("omitted candidate");
                    retained.insert(position, candidate);
                    continue;
                }
                Err(err) => return Err(err),
            };
            let dual = master
                .dual()
                .map_err(|e| Error::Numerical(format!("restricted dual: {e}")))?;
            let restricted_bound = -master.b.iter().zip(&dual).map(|(b, y)| b * y).sum::<f64>();
            let epigraph_dual = master
                .rows
                .iter()
                .zip(&dual)
                .map(|(row, y)| row.epigraph() * y)
                .sum::<f64>();
            if !restricted_bound.is_finite()
                || epigraph_dual < -1.0 - CHECK
                || (policy.worst_cost / master.cost_unit - restricted_bound).abs() > CHECK
            {
                return Err(Error::Numerical(
                    "restricted dual does not certify its primal value".into(),
                ));
            }
            // If floating-point error put sum of cost multipliers above 1,
            // shrink all dual variables enough to make the epigraph column feasible.
            let factor = (-epigraph_dual).max(1.0);
            let mut best = None;
            let mut correction = 0.0;
            for (g, eligible) in all.iter().enumerate() {
                let mut min_rc = 0.0_f64;
                for &j in eligible {
                    let reduced = master
                        .rows
                        .iter()
                        .zip(&dual)
                        .map(|(row, y)| {
                            row.coefficient(self, g, j, req.epsilon(), master.cost_unit)
                                * (y / factor)
                        })
                        .sum::<f64>();
                    if !reduced.is_finite() {
                        return Err(Error::Numerical("non-finite reduced cost".into()));
                    }
                    min_rc = min_rc.min(reduced);
                    if retained.binary_search(&j).is_err()
                        && reduced * master.cost_unit < -tolerance
                        && reduced < -1e-10
                        && best.is_none_or(|(value, _)| reduced < value)
                    {
                        best = Some((reduced, j));
                    }
                }
                correction -= min_rc;
            }
            if let Some((_, index)) = best {
                let position = retained.binary_search(&index).expect_err("omitted column");
                retained.insert(position, index);
            } else {
                let lower = (restricted_bound / factor - correction) * master.cost_unit;
                return Ok(PricedPolicy {
                    gap_bound: (policy.worst_cost - lower).max(0.0),
                    policy,
                    iterations,
                    retained,
                });
            }
        }
        Err(Error::Numerical(
            "column generation exceeded candidate count".into(),
        ))
    }

    fn price_feasibility(
        &self,
        req: SelectionRequirements,
        master: &Master,
        all: &[Vec<usize>],
        retained: &[usize],
    ) -> Result<usize, Error> {
        let rows = master
            .rows
            .iter()
            .zip(&master.a)
            .zip(&master.b)
            .filter_map(|((row, coefficients), rhs)| {
                matches!(row, Row::Limit(..) | Row::GroupSum(..)).then_some((
                    *row,
                    coefficients,
                    *rhs,
                ))
            })
            .collect::<Vec<_>>();
        let slack_count = rows
            .iter()
            .filter(|(row, _, _)| matches!(row, Row::Limit(..)))
            .count();
        let n = master.columns.len();
        let mut slack = n;
        let mut a = Vec::with_capacity(rows.len());
        let mut b = Vec::with_capacity(rows.len());
        for &(row, coefficients, rhs) in &rows {
            let mut coefficients = coefficients[..n].to_vec();
            coefficients.resize(n + slack_count, 0.0);
            if matches!(row, Row::Limit(..)) {
                coefficients[slack] = -1.0;
                slack += 1;
            }
            a.push(coefficients);
            b.push(rhs);
        }
        let mut objective = vec![0.0; n];
        objective.resize(n + slack_count, 1.0);
        let dual = solve_dual(&a, &b, &objective)?;
        let restricted_lower = -b.iter().zip(&dual).map(|(rhs, y)| rhs * y).sum::<f64>();
        let mut best: Option<(f64, usize)> = None;
        let mut correction = 0.0;
        for (g, eligible) in all.iter().enumerate() {
            let mut minimum = 0.0_f64;
            for &j in eligible {
                let reduced = rows
                    .iter()
                    .zip(&dual)
                    .map(|((row, _, _), y)| {
                        row.coefficient(self, g, j, req.epsilon(), master.cost_unit) * y
                    })
                    .sum::<f64>();
                minimum = minimum.min(reduced);
                if reduced < -1e-10
                    && retained.binary_search(&j).is_err()
                    && best.is_none_or(|(value, _)| reduced < value)
                {
                    best = Some((reduced, j));
                }
            }
            correction -= minimum;
        }
        if let Some((_, j)) = best {
            return Ok(j);
        }
        // Allow for small dual residuals before ruling out the full pool.
        if restricted_lower - correction > CHECK {
            return Err(Error::Infeasible);
        }
        let full = self.solve_choices(req, all, Objective::Cost, None)?;
        full.weights
            .iter()
            .flatten()
            .copied()
            .enumerate()
            .map(|(index, weight)| (index % self.candidates.len(), weight))
            .find(|(j, weight)| *weight > 0.0 && retained.binary_search(j).is_err())
            .map(|(j, _)| j)
            .ok_or_else(|| Error::Numerical("unable to recover a feasible restricted pool".into()))
    }
}

impl ServiceReport {
    fn from_cells(cells: impl Iterator<Item = (f64, Cell)>, epsilon: f64) -> Self {
        let mut report = Self::default();
        let mut failure = Some(0.0);
        for (weight, cell) in cells {
            let m = cell.moments();
            report.acceptance += weight * m.acceptance;
            report.joint_defect += weight * m.joint_defect;
            report.cost += weight * m.cost;
            report.lateness += weight * m.lateness;
            let score = if epsilon == 1.0 {
                risk(cell, epsilon)
            } else {
                // A report outside the constrained population may have been
                // calibrated at a different epsilon. Use its separate bounds.
                cell.risk(epsilon)
                    .unwrap_or(m.joint_defect - epsilon * m.acceptance)
            };
            report.risk_score += weight * score;
            failure = failure.zip(cell.failure()).map(|(sum, f)| sum + weight * f);
        }
        report.failure = failure.map(|f| f.clamp(0.0, 1.0));
        report.defect_rate =
            (report.acceptance > 0.0).then(|| report.joint_defect / report.acceptance);
        report
    }
}
