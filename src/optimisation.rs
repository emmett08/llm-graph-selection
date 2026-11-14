use crate::{simplex, Cell, Error, Requirements, ServiceScope, Study};

const CHECK: f64 = 1e-8;

#[derive(Debug, Clone, Copy, Default)]
pub struct ServiceReport {
    pub acceptance: f64,
    pub joint_defect: f64,
    pub cost: f64,
    pub lateness: f64,
    /// Ratio of the supplied moments, if acceptance is positive.
    /// With bounds, `risk_score` is the actual service constraint.
    pub defect_rate: Option<f64>,
    /// Joint defect minus tolerance times acceptance, or its supplied upper bound.
    pub risk_score: f64,
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub worst_cost: f64,
    /// Weights indexed by group and candidate.
    pub weights: Vec<Vec<f64>>,
    pub active_columns: usize,
    pub scenarios: Vec<ServiceReport>,
    /// Service indexed by group and scenario.
    pub groups: Vec<Vec<ServiceReport>>,
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

#[derive(Clone, Copy)]
enum Row {
    Cost(usize),
    Risk(usize, Option<usize>),
    Completion(usize, Option<usize>),
    Late(usize, Option<usize>),
    GroupSum(usize, f64),
    Budget(f64),
}

struct Master {
    columns: Vec<(usize, usize)>,
    rows: Vec<Row>,
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    cost_unit: f64,
}

impl Row {
    fn coefficient(self, study: &Study, g: usize, j: usize, epsilon: f64, unit: f64) -> f64 {
        let weighted = |k: usize, group: Option<usize>| match group {
            Some(index) => f64::from(g == index),
            None => study.proportions[k][g],
        };
        match self {
            Self::Cost(k) => study.proportions[k][g] * (study.at(g, j, k).moments().cost / unit),
            Self::Risk(k, group) => weighted(k, group) * risk(study.at(g, j, k), epsilon),
            Self::Completion(k, group) => {
                -weighted(k, group) * study.at(g, j, k).moments().acceptance
            }
            Self::Late(k, group) => weighted(k, group) * study.at(g, j, k).moments().lateness,
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
            Self::Cost(_) => -1.0,
            Self::Budget(_) => 1.0,
            _ => 0.0,
        }
    }

    fn rhs(self, requirements: Requirements, unit: f64) -> f64 {
        match self {
            Self::Completion(..) => -requirements.completion,
            Self::Late(..) => requirements.lateness,
            Self::GroupSum(_, sign) => sign,
            Self::Budget(budget) => budget / unit,
            _ => 0.0,
        }
    }
}

/// The constraint at epsilon=1 follows directly from Y <= A, even with bounds.
fn risk(cell: Cell, epsilon: f64) -> f64 {
    if epsilon == 1.0 && !matches!(cell, Cell::Point(_)) {
        return 0.0;
    }
    cell.risk(epsilon)
        .expect("risk tolerance validated before constructing LP")
}

impl Master {
    fn new(study: &Study, req: Requirements, choices: &[Vec<usize>], budget: Option<f64>) -> Self {
        let columns: Vec<_> = choices
            .iter()
            .enumerate()
            .flat_map(|(g, candidates)| candidates.iter().map(move |&j| (g, j)))
            .collect();
        let cost_max = study
            .cells
            .iter()
            .map(|cell| cell.moments().cost)
            .fold(budget.unwrap_or(0.0), f64::max);
        let cost_unit = if cost_max > 0.0 { cost_max } else { 1.0 };
        let mut rows = Vec::new();
        for (k, proportions) in study.proportions.iter().enumerate() {
            rows.push(Row::Cost(k));
            let mut add_limits = |group| {
                if req.epsilon < 1.0 {
                    rows.push(Row::Risk(k, group));
                }
                rows.push(Row::Completion(k, group));
                rows.push(Row::Late(k, group));
            };
            match req.scope {
                ServiceScope::Scenario => add_limits(None),
                ServiceScope::Group => {
                    for (g, probability) in proportions.iter().enumerate() {
                        if *probability > 0.0 {
                            add_limits(Some(g));
                        }
                    }
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
        let a = rows
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|&(g, j)| row.coefficient(study, g, j, req.epsilon, cost_unit))
                    .chain(std::iter::once(row.epigraph()))
                    .collect()
            })
            .collect();
        let b = rows.iter().map(|row| row.rhs(req, cost_unit)).collect();
        Self {
            columns,
            rows,
            a,
            b,
            cost_unit,
        }
    }

    fn solve(&self, study: &Study, req: Requirements) -> Result<(Policy, Vec<f64>), Error> {
        let mut c = vec![0.0; self.columns.len() + 1];
        c[self.columns.len()] = -1.0;
        let solution = solve_lp(&self.a, &self.b, &c)?;
        let weights = self.weights(study, &solution)?;
        let policy = study.report(weights, req, self.columns.len())?;
        if ((policy.worst_cost / self.cost_unit) - solution[self.columns.len()]).abs() > CHECK {
            return Err(Error::Numerical(
                "epigraph and recovered policy cost disagree".into(),
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
        // For min z with Ax <= b, x >= 0, the Lagrangian lower bound is
        // max -b^T y with -A^T y <= objective and y >= 0.
        let n = self.columns.len() + 1;
        let a: Vec<Vec<f64>> = (0..n)
            .map(|j| self.a.iter().map(|row| -row[j]).collect())
            .collect();
        let mut b = vec![0.0; n];
        b[n - 1] = 1.0;
        let c: Vec<_> = self.b.iter().map(|b| -b).collect();
        solve_lp(&a, &b, &c)
    }
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

impl Study {
    fn choices(&self, req: Requirements, prune: bool) -> Vec<Vec<usize>> {
        (0..self.groups.len())
            .map(|g| {
                let all = 0..self.candidates.len();
                if !prune {
                    return all.collect();
                }
                let columns: Vec<Vec<f64>> = all
                    .clone()
                    .map(|j| {
                        (0..self.scenarios.len())
                            .filter(|&k| self.proportions[k][g] > 0.0)
                            .flat_map(|k| {
                                let cell = self.at(g, j, k);
                                let m = cell.moments();
                                [m.cost, risk(cell, req.epsilon), -m.acceptance, m.lateness]
                            })
                            .collect()
                    })
                    .collect();
                all.filter(|&j| {
                    !columns.iter().enumerate().any(|(i, left)| {
                        i != j
                            && left.iter().zip(&columns[j]).all(|(a, b)| a <= b)
                            && (i < j || left != &columns[j])
                    })
                })
                .collect()
            })
            .collect()
    }

    /// Minimise worst-scenario mean cost at a fixed defect tolerance.
    pub fn solve(&self, req: Requirements, prune: bool) -> Result<Policy, Error> {
        req.validate()?;
        self.check_risk(req.epsilon)?;
        let choices = self.choices(req, prune);
        Ok(Master::new(self, req, &choices, None).solve(self, req)?.0)
    }

    /// Evaluate a routing rule without imposing the service thresholds.
    pub fn evaluate_policy(&self, weights: &[Vec<f64>], epsilon: f64) -> Result<Policy, Error> {
        if !crate::probability(epsilon) || weights.len() != self.groups.len() {
            return Err(Error::InvalidInput(
                "invalid tolerance or policy dimensions".into(),
            ));
        }
        self.check_risk(epsilon)?;
        if weights.iter().any(|row| {
            row.len() != self.candidates.len()
                || row.iter().any(|&w| !crate::probability(w))
                || (row.iter().sum::<f64>() - 1.0).abs() > 1e-9
        }) {
            return Err(Error::InvalidInput(
                "each policy row must be a probability distribution".into(),
            ));
        }
        Ok(self.summarise(
            weights.to_vec(),
            epsilon,
            self.groups.len() * self.candidates.len(),
        ))
    }

    fn summarise(&self, weights: Vec<Vec<f64>>, epsilon: f64, active_columns: usize) -> Policy {
        let mut groups =
            vec![vec![ServiceReport::default(); self.scenarios.len()]; self.groups.len()];
        let mut scenarios = vec![ServiceReport::default(); self.scenarios.len()];
        for (g, (row, group)) in weights.iter().zip(&mut groups).enumerate() {
            for (k, (report, scenario)) in group.iter_mut().zip(&mut scenarios).enumerate() {
                for (j, &w) in row.iter().enumerate() {
                    let cell = self.at(g, j, k);
                    let m = cell.moments();
                    report.acceptance += w * m.acceptance;
                    report.joint_defect += w * m.joint_defect;
                    report.cost += w * m.cost;
                    report.lateness += w * m.lateness;
                    report.risk_score += w * risk(cell, epsilon);
                }
                report.set_ratio();
                let mu = self.proportions[k][g];
                scenario.acceptance += mu * report.acceptance;
                scenario.joint_defect += mu * report.joint_defect;
                scenario.cost += mu * report.cost;
                scenario.lateness += mu * report.lateness;
                scenario.risk_score += mu * report.risk_score;
            }
        }
        for scenario in &mut scenarios {
            scenario.set_ratio();
        }
        let worst_cost = scenarios.iter().map(|s| s.cost).fold(0.0, f64::max);
        Policy {
            worst_cost,
            weights,
            active_columns,
            scenarios,
            groups,
        }
    }

    fn report(
        &self,
        weights: Vec<Vec<f64>>,
        req: Requirements,
        active_columns: usize,
    ) -> Result<Policy, Error> {
        let policy = self.summarise(weights, req.epsilon, active_columns);
        let violated = |r: &ServiceReport| {
            r.acceptance <= 0.0
                || r.acceptance + CHECK < req.completion
                || r.lateness > req.lateness + CHECK
                || r.risk_score > CHECK
        };
        let failure = match req.scope {
            ServiceScope::Scenario => policy.scenarios.iter().any(violated),
            ServiceScope::Group => policy.groups.iter().enumerate().any(|(g, reports)| {
                reports
                    .iter()
                    .enumerate()
                    .any(|(k, r)| self.proportions[k][g] > 0.0 && violated(r))
            }),
        };
        if failure || !policy.worst_cost.is_finite() {
            return Err(Error::Numerical(
                "recovered policy violates service or cost bounds".into(),
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
            let choices = self.choices(r, false);
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
            || self
                .cells
                .iter()
                .any(|cell| matches!(cell, Cell::TimelyQueue { .. }))
        {
            return Err(Error::InvalidInput("one-LP risk minimisation requires one scenario, scenario service and point or separate endpoint moments".into()));
        }
        let master = Master::new(
            self,
            Requirements {
                epsilon: 1.0,
                ..req
            },
            &self.choices(req, false),
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
            },
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
            },
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
        self.check_risk(req.epsilon)?;
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
        for iterations in 1..=self.candidates.len() {
            let choices = vec![retained.clone(); self.groups.len()];
            let master = Master::new(self, req, &choices, None);
            let (policy, _) = master.solve(self, req)?;
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
            for g in 0..self.groups.len() {
                let mut min_rc = 0.0_f64;
                for j in 0..self.candidates.len() {
                    let reduced = master
                        .rows
                        .iter()
                        .zip(&dual)
                        .map(|(row, y)| {
                            row.coefficient(self, g, j, req.epsilon, master.cost_unit)
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
}

impl ServiceReport {
    fn set_ratio(&mut self) {
        self.defect_rate = (self.acceptance > 0.0).then(|| self.joint_defect / self.acceptance);
    }
}
