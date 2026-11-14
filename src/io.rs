//! CSV input for the routing study.
//!
//! Each loader expects a header. Columns can appear in any order and extra
//! columns are ignored. Fields can be quoted with double quotes. Quoted
//! fields may contain commas and doubled quotes, but must fit on one line.
//! Axis order comes from the first occurrence of each ID in the data file.

use crate::calibration::{CalibrationCell, CalibrationMethod};
use crate::outcomes::{cell_from_outcomes, TerminalOutcome};
use crate::{Cell, Error, Moments, Study};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;

pub use crate::outcomes::Timing;

/// Read moment rows and scenario weights.
///
/// Data columns are `group,scenario,candidate,acceptance,joint_defect,cost_usd,lateness`.
/// Weight columns are `scenario,group,weight`. Each file must cover its full
/// Cartesian product exactly once.
pub fn load_moments(
    path: impl AsRef<Path>,
    scenario_path: impl AsRef<Path>,
) -> Result<Study, Error> {
    let csv = Csv::read(path.as_ref())?;
    let axes = Axes::from_csv(&csv)?;
    let fields = csv.fields(["acceptance", "joint_defect", "cost_usd", "lateness"])?;
    let mut cells = vec![None; axes.checked_size(csv.rows.len())?];
    for row in &csv.rows {
        let i = axes.index(row);
        let [acceptance, joint_defect, cost, lateness] = row.values(fields)?;
        let cell = Cell::Point(Moments {
            acceptance: clamp_upper_ulp(acceptance),
            joint_defect: clamp_upper_ulp(joint_defect),
            cost,
            lateness: clamp_upper_ulp(lateness),
        });
        insert(&mut cells, i, cell, row)?;
    }
    axes.finish(cells, scenario_path.as_ref())
}

/// Read calibration summaries and calibrate all uncertain cells as one family.
///
/// Data columns are `group,scenario,candidate,attempts,accepted,accepted_defects,late,total_cost,cost_cap`.
/// A row with zero attempts declares known zero outcomes. Its remaining
/// numeric fields must all be zero. Use point moments for other known outcomes.
pub fn load_calibration(
    path: impl AsRef<Path>,
    scenario_path: impl AsRef<Path>,
    alpha: f64,
    method: CalibrationMethod,
) -> Result<Study, Error> {
    if !(0.0 < alpha && alpha < 1.0) {
        return Err(invalid(
            "family error alpha must lie strictly between zero and one",
        ));
    }
    match method {
        CalibrationMethod::SeparateHoeffding => (),
        CalibrationMethod::PairedHoeffding { epsilon }
        | CalibrationMethod::PairedEmpiricalBernstein { epsilon } => {
            if !(0.0..=1.0).contains(&epsilon) {
                return Err(invalid(
                    "paired calibration epsilon must lie between zero and one",
                ));
            }
        }
    }

    let csv = Csv::read(path.as_ref())?;
    let axes = Axes::from_csv(&csv)?;
    let counts = csv.fields(["attempts", "accepted", "accepted_defects", "late"])?;
    let costs = csv.fields(["total_cost", "cost_cap"])?;
    let mut tallies = vec![None; axes.checked_size(csv.rows.len())?];
    for row in &csv.rows {
        let [attempts, accepted, accepted_defects, late] = row.values(counts)?;
        let [total_cost, cost_cap] = row.values(costs)?;
        let summary = CalibrationCell {
            attempts,
            accepted,
            accepted_defects,
            late,
            total_cost,
            cost_cap,
        };
        insert(&mut tallies, axes.index(row), summary, row)?;
    }

    let uncertain_cells = tallies
        .iter()
        .flatten()
        .filter(|cell| cell.attempts > 0)
        .count();
    let mut cells = Vec::with_capacity(tallies.len());
    for (i, summary) in tallies.into_iter().enumerate() {
        let summary = summary.ok_or_else(|| axes.missing_cell(i))?;
        let cell = if summary.attempts == 0 {
            if summary.accepted != 0
                || summary.accepted_defects != 0
                || summary.late != 0
                || summary.total_cost != 0.0
                || summary.cost_cap != 0.0
            {
                return Err(invalid(format!(
                    "known zero-outcome cell {} has nonzero data",
                    axes.cell_name(i)
                )));
            }
            Cell::Point(Moments {
                acceptance: 0.0,
                joint_defect: 0.0,
                cost: 0.0,
                lateness: 0.0,
            })
        } else {
            let calibrated =
                summary
                    .calibrate(alpha, uncertain_cells, method)
                    .map_err(|error| {
                        invalid(format!("calibration cell {}: {error}", axes.cell_name(i)))
                    })?;
            Cell::Conservative {
                bounds: calibrated.conservative,
                paired_risk: calibrated.risk,
            }
        };
        cells.push(Some(cell));
    }
    axes.finish(cells, scenario_path.as_ref())
}

/// Read the terminal distribution in each cell and derive its service moments.
///
/// Data columns are `group,scenario,candidate,probability,accepted,defective,cost_usd,duration_seconds,queued`.
/// Boolean fields accept `0` or `false`, and `1` or `true`. Each cell has
/// one or more rows and its row probabilities must sum to one. Repeated
/// terminal states represent alternative paths with the same observable results.
pub fn load_outcomes(
    path: impl AsRef<Path>,
    scenario_path: impl AsRef<Path>,
    deadline: f64,
    timing: Timing,
) -> Result<Study, Error> {
    let csv = Csv::read(path.as_ref())?;
    let axes = Axes::from_csv(&csv)?;
    let numbers = csv.fields(["probability", "cost_usd", "duration_seconds"])?;
    let [accepted, defective, queued] = csv.fields(["accepted", "defective", "queued"])?;
    let mut distributions = vec![Vec::new(); axes.checked_size(csv.rows.len())?];
    for row in &csv.rows {
        let [probability, cost, duration] = row.values(numbers)?;
        distributions[axes.index(row)].push(TerminalOutcome {
            probability: clamp_upper_ulp(probability),
            accepted: row.flag(accepted)?,
            defective: row.flag(defective)?,
            cost,
            duration,
            queued: row.flag(queued)?,
        });
    }
    let mut cells = Vec::with_capacity(distributions.len());
    for (i, distribution) in distributions.iter().enumerate() {
        if distribution.is_empty() {
            return Err(axes.missing_cell(i));
        }
        let cell = cell_from_outcomes(distribution, deadline, timing)
            .map_err(|error| invalid(format!("terminal cell {}: {error}", axes.cell_name(i))))?;
        cells.push(Some(cell));
    }
    axes.finish(cells, scenario_path.as_ref())
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

// Finite summation may round an exact unit probability slightly above one.
// Permit only a few ulps on file input. Study::new validates all supplied data.
fn clamp_upper_ulp(x: f64) -> f64 {
    if x > 1.0 && x <= 1.0 + 4.0 * f64::EPSILON {
        1.0
    } else {
        x
    }
}

fn insert<T>(items: &mut [Option<T>], index: usize, value: T, row: &Row) -> Result<(), Error> {
    if items[index].is_some() {
        return Err(row.error("duplicate cell"));
    }
    items[index] = Some(value);
    Ok(())
}

struct Axis {
    names: Vec<String>,
    index: HashMap<String, usize>,
}

impl Axis {
    fn new() -> Self {
        Self {
            names: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn record(&mut self, name: &str) {
        if !self.index.contains_key(name) {
            self.index.insert(name.to_owned(), self.names.len());
            self.names.push(name.to_owned());
        }
    }

    fn get(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }
}

struct Axes {
    group: Axis,
    candidate: Axis,
    scenario: Axis,
    fields: [usize; 3],
}

impl Axes {
    fn from_csv(csv: &Csv) -> Result<Self, Error> {
        let fields = csv.fields(["group", "candidate", "scenario"])?;
        let mut axes = Self {
            group: Axis::new(),
            candidate: Axis::new(),
            scenario: Axis::new(),
            fields,
        };
        for row in &csv.rows {
            let [g, j, k] = fields.map(|i| row.fields[i].as_str());
            if [g, j, k].into_iter().any(|id| id.trim().is_empty()) {
                return Err(row.error("group, candidate and scenario IDs must be non-empty"));
            }
            axes.group.record(g);
            axes.candidate.record(j);
            axes.scenario.record(k);
        }
        Ok(axes)
    }

    fn checked_size(&self, rows: usize) -> Result<usize, Error> {
        let size = self
            .group
            .names
            .len()
            .checked_mul(self.candidate.names.len())
            .and_then(|value| value.checked_mul(self.scenario.names.len()))
            .ok_or_else(|| invalid("too many cells for this platform"))?;
        if rows < size {
            return Err(invalid(format!(
                "missing cell data: {rows} rows cannot cover {size} group, candidate and scenario combinations"
            )));
        }
        Ok(size)
    }

    fn index(&self, row: &Row) -> usize {
        let [g, j, k] = self.fields.map(|i| row.fields[i].as_str());
        let g = self.group.index[g];
        let j = self.candidate.index[j];
        let k = self.scenario.index[k];
        (g * self.candidate.names.len() + j) * self.scenario.names.len() + k
    }

    fn cell_name(&self, i: usize) -> String {
        let k = i % self.scenario.names.len();
        let gj = i / self.scenario.names.len();
        let j = gj % self.candidate.names.len();
        let g = gj / self.candidate.names.len();
        format!(
            "group {:?}, candidate {:?}, scenario {:?}",
            self.group.names[g], self.candidate.names[j], self.scenario.names[k]
        )
    }

    fn missing_cell(&self, i: usize) -> Error {
        invalid(format!("missing cell for {}", self.cell_name(i)))
    }

    fn finish(self, cells: Vec<Option<Cell>>, scenario_path: &Path) -> Result<Study, Error> {
        let cells = cells
            .into_iter()
            .enumerate()
            .map(|(i, cell)| cell.ok_or_else(|| self.missing_cell(i)))
            .collect::<Result<Vec<_>, _>>()?;
        let proportions = self.load_weights(scenario_path)?;
        Study::new(
            self.group.names,
            self.candidate.names,
            self.scenario.names,
            proportions,
            cells,
        )
    }

    fn load_weights(&self, path: &Path) -> Result<Vec<Vec<f64>>, Error> {
        let csv = Csv::read(path)?;
        let [scenario, group, weight] = csv.fields(["scenario", "group", "weight"])?;
        let mut proportions = vec![vec![None; self.group.names.len()]; self.scenario.names.len()];
        for row in &csv.rows {
            let group_name = &row.fields[group];
            let scenario_name = &row.fields[scenario];
            let g = self
                .group
                .get(group_name)
                .ok_or_else(|| row.error(format!("unknown group {group_name:?}")))?;
            let k = self
                .scenario
                .get(scenario_name)
                .ok_or_else(|| row.error(format!("unknown scenario {scenario_name:?}")))?;
            insert(
                &mut proportions[k],
                g,
                clamp_upper_ulp(row.value(weight)?),
                row,
            )?;
        }
        proportions
            .into_iter()
            .enumerate()
            .map(|(k, group_rows)| {
                group_rows
                    .into_iter()
                    .enumerate()
                    .map(|(g, value)| {
                        value.ok_or_else(|| {
                            invalid(format!(
                                "missing weight for group {:?}, scenario {:?}",
                                self.group.names[g], self.scenario.names[k]
                            ))
                        })
                    })
                    .collect()
            })
            .collect()
    }
}

struct Row {
    line: usize,
    fields: Vec<String>,
}

impl Row {
    fn error(&self, cause: impl Display) -> Error {
        invalid(format!("CSV line {}: {cause}", self.line))
    }

    fn value<T>(&self, column: usize) -> Result<T, Error>
    where
        T: FromStr,
    {
        self.fields[column].parse().map_err(|_| {
            self.error(format!(
                "invalid value {:?} in column {}",
                self.fields[column],
                column + 1
            ))
        })
    }

    fn values<T, const N: usize>(&self, columns: [usize; N]) -> Result<[T; N], Error>
    where
        T: FromStr + Default,
    {
        let mut array = std::array::from_fn(|_| T::default());
        for (value, column) in array.iter_mut().zip(columns) {
            *value = self.value(column)?;
        }
        Ok(array)
    }

    fn flag(&self, column: usize) -> Result<bool, Error> {
        match self.fields[column].as_str() {
            "0" => Ok(false),
            "1" => Ok(true),
            other => Err(self.error(format!("expected 0 or 1, got {other:?}"))),
        }
    }
}

struct Csv {
    header: Vec<String>,
    rows: Vec<Row>,
}

impl Csv {
    fn read(path: &Path) -> Result<Self, Error> {
        let file =
            File::open(path).map_err(|error| invalid(format!("{}: {error}", path.display())))?;
        let mut header: Option<Vec<String>> = None;
        let mut rows = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|error| invalid(format!("{}: {error}", path.display())))?;
            if line.trim().is_empty() {
                continue;
            }
            let fields = parse_record(line.trim_start_matches('\u{feff}')).map_err(|error| {
                invalid(format!("{} line {}: {error}", path.display(), index + 1))
            })?;
            match &header {
                Some(header_fields) => {
                    if fields.len() != header_fields.len() {
                        return Err(invalid(format!(
                            "{} line {} has {} fields, expected {}",
                            path.display(),
                            index + 1,
                            fields.len(),
                            header_fields.len()
                        )));
                    }
                    rows.push(Row {
                        line: index + 1,
                        fields,
                    });
                }
                None => {
                    let unique: HashSet<_> = fields.iter().collect();
                    if unique.len() != fields.len() || fields.iter().any(String::is_empty) {
                        return Err(invalid(format!(
                            "{} has empty or repeated headers",
                            path.display()
                        )));
                    }
                    header = Some(fields);
                }
            }
        }
        let header =
            header.ok_or_else(|| invalid(format!("{} has no CSV header", path.display())))?;
        if rows.is_empty() {
            return Err(invalid(format!("{} has no data rows", path.display())));
        }
        Ok(Self { header, rows })
    }

    fn fields<const N: usize>(&self, headers: [&str; N]) -> Result<[usize; N], Error> {
        let mut columns = [0; N];
        for (column, name) in columns.iter_mut().zip(headers) {
            *column = self
                .header
                .iter()
                .position(|field| field == name)
                .ok_or_else(|| invalid(format!("missing CSV header {name:?}")))?;
        }
        Ok(columns)
    }
}

// Quoted CSV records are parsed a line at a time to keep source locations
// useful. Spaces outside a field are ignored.
fn parse_record(line: &str) -> Result<Vec<String>, &'static str> {
    let mut chars = line.chars().peekable();
    let mut fields = Vec::new();
    loop {
        while chars.peek().is_some_and(|ch| ch.is_whitespace()) {
            chars.next();
        }
        let mut field = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            loop {
                match chars.next() {
                    Some('"') if chars.peek() == Some(&'"') => {
                        chars.next();
                        field.push('"');
                    }
                    Some('"') => break,
                    Some(ch) => field.push(ch),
                    None => return Err("unterminated quoted field"),
                }
            }
            while chars.peek().is_some_and(|ch| ch.is_whitespace()) {
                chars.next();
            }
        } else {
            while chars.peek().is_some_and(|ch| *ch != ',') {
                if let Some(ch) = chars.next() {
                    if ch == '"' {
                        return Err("unexpected quote in unquoted field");
                    }
                    field.push(ch);
                }
            }
            field.truncate(field.trim_end().len());
        }
        fields.push(field);
        match chars.next() {
            Some(',') => (),
            None => return Ok(fields),
            Some(_) => return Err("unexpected text after quoted field"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_record;

    #[test]
    fn csv_record_handles_quoted_delimiters_and_empty_fields() {
        assert_eq!(
            parse_record("a, \"b,c\", \"d\"\"e\",").unwrap(),
            ["a", "b,c", "d\"e", ""]
        );
        assert!(parse_record("a,\"b").is_err());
        assert!(parse_record("a,\"b\"c").is_err());
        assert!(parse_record("a,b\"c\"").is_err());
    }
}
