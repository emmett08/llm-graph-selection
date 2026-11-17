use llm_workflow_optimiser::{
    calibration::CalibrationMethod,
    io::{
        load_calibration, load_failure_calibration, load_families, load_moments, load_outcomes,
        Timing,
    },
    Error, Policy, Requirements, RoutingRestriction, SelectionRequirements, ServiceScope, Study,
};
use std::collections::HashMap;
use std::process::ExitCode;

const HELP: &str =
    "llm-workflow-optimiser <cost|failure|cost-gap|risk|risk-one-lp|price> <input.csv> <scenarios.csv> [options]

Commands
  cost         Minimise worst-scenario cost under the service limits
  failure      Minimise worst-scenario task failure at a fixed budget
  cost-gap     Compare conservative and optimistic cost bounds after calibration
  risk         Minimise worst-scenario defect risk at a fixed budget
  risk-one-lp  Minimise defect risk with one scenario using Charnes–Cooper
  price        Price the finite candidate pool, starting from candidate seed IDs

Service options (defaults in parentheses)
  --failure <probability>       Task failure ceiling in each scenario and selected group
  --epsilon <probability>       Accepted-output defect limit (0.10)
  --completion <probability>    Completion floor (0.80)
  --lateness <probability>      Lateness ceiling (0.15)
  --scope <scenario|group>      Apply service limits by scenario or observed group (scenario)
  --prune                       Screen dominated columns (cost, failure, cost-gap)
  --budget <cost>               Worst-scenario cost ceiling (failure and risk commands)
  --tolerance <number>          Bisection width (1e-7) or pricing tolerance in cost units (1e-10)
  --seed <id,id,...>             Initial candidate set (price only)

Routing options
  --routing <mixed|family|fixed>  Routing across families, within one family or one configuration (cost, failure, cost-gap)
  --families <csv>               Candidate and family columns (family mode)
  --exclude <id,id,...>          Exclude configurations from all groups (cost, failure, cost-gap, price)
  Set --epsilon, --completion or --lateness to none to omit that limit.
  A conditional defect ceiling requires a positive completion floor.

Input options
  --input <moments|outcomes|calibration|failure-calibration>  Input format (moments)
  --alpha <probability>         Family error probability (0.05)
  --method <separate|paired|bernstein>    Method for --input calibration (separate)
  --deadline <seconds>          Deadline for outcomes (180)
  --timing <observed|timely|queue|timely-queue>  Outcome definition (observed)
  --delay <seconds>             Accumulated queue delay upper bound (0)
  --exceedance <probability>    Cell probability of exceeding the bound (0, queue only)
";

struct Options<'a> {
    values: HashMap<&'a str, &'a str>,
    prune: bool,
}

impl<'a> Options<'a> {
    fn parse(args: &'a [String]) -> Result<Self, Error> {
        let mut options = Self {
            values: HashMap::new(),
            prune: false,
        };
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            let flag = flag.as_str();
            if flag == "--prune" {
                if options.prune {
                    return Err(invalid("duplicate --prune"));
                }
                options.prune = true;
                continue;
            }
            if !matches!(
                flag,
                "--epsilon"
                    | "--completion"
                    | "--lateness"
                    | "--scope"
                    | "--budget"
                    | "--tolerance"
                    | "--seed"
                    | "--input"
                    | "--alpha"
                    | "--method"
                    | "--deadline"
                    | "--timing"
                    | "--delay"
                    | "--exceedance"
                    | "--failure"
                    | "--routing"
                    | "--families"
                    | "--exclude"
            ) {
                return Err(invalid(format!("unknown option {flag}")));
            }
            let value = args
                .next()
                .ok_or_else(|| invalid(format!("{flag} needs a value")))?;
            if value.starts_with("--") {
                return Err(invalid(format!("{flag} needs a value")));
            }
            if options.values.insert(flag, value).is_some() {
                return Err(invalid(format!("duplicate option {flag}")));
            }
        }
        Ok(options)
    }

    fn value(&self, name: &str, default: &'a str) -> &'a str {
        self.values.get(name).copied().unwrap_or(default)
    }

    fn number(&self, name: &str, default: f64) -> Result<f64, Error> {
        Ok(self.optional_number(name)?.unwrap_or(default))
    }

    fn optional_number(&self, name: &str) -> Result<Option<f64>, Error> {
        self.values
            .get(name)
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| invalid(format!("{name} needs a number")))
            })
            .transpose()
    }

    fn limit(&self, name: &str, default: f64) -> Result<Option<f64>, Error> {
        if self.values.get(name) == Some(&"none") {
            Ok(None)
        } else {
            self.number(name, default).map(Some)
        }
    }

    fn uses_selection_options(&self) -> bool {
        ["--failure", "--routing", "--families", "--exclude"]
            .iter()
            .any(|name| self.values.contains_key(name))
            || ["--completion", "--epsilon", "--lateness"]
                .iter()
                .any(|name| self.values.get(name) == Some(&"none"))
            || self.value("--input", "moments") == "failure-calibration"
    }

    fn reject(&self, names: &[&str]) -> Result<(), Error> {
        if let Some(name) = names.iter().find(|name| self.values.contains_key(**name)) {
            return Err(invalid(format!(
                "{name} does not apply to this command or input"
            )));
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn candidate_id(study: &Study, id: &str, option: &str) -> Result<usize, Error> {
    study
        .candidates()
        .iter()
        .position(|candidate| candidate == id)
        .ok_or_else(|| invalid(format!("unknown {option} candidate {id}")))
}

fn seed_indices(study: &Study, options: &Options<'_>) -> Result<Vec<usize>, Error> {
    let names = options
        .values
        .get("--seed")
        .ok_or_else(|| invalid("price requires --seed"))?;
    names
        .split(',')
        .map(|id| candidate_id(study, id, "seed"))
        .collect()
}

fn study(args: &[String], options: &Options<'_>, defect: Option<f64>) -> Result<Study, Error> {
    match options.value("--input", "moments") {
        "moments" => {
            options.reject(&[
                "--alpha",
                "--method",
                "--deadline",
                "--timing",
                "--delay",
                "--exceedance",
            ])?;
            load_moments(&args[1], &args[2])
        }
        "calibration" => {
            options.reject(&["--deadline", "--timing", "--delay", "--exceedance"])?;
            let method = match options.value("--method", "separate") {
                "separate" => CalibrationMethod::SeparateHoeffding,
                "paired" => CalibrationMethod::PairedHoeffding {
                    epsilon: defect
                        .ok_or_else(|| invalid("paired calibration needs a defect limit"))?,
                },
                "bernstein" => CalibrationMethod::PairedEmpiricalBernstein {
                    epsilon: defect
                        .ok_or_else(|| invalid("paired calibration needs a defect limit"))?,
                },
                _ => return Err(invalid("unknown calibration method")),
            };
            load_calibration(&args[1], &args[2], options.number("--alpha", 0.05)?, method)
        }
        "failure-calibration" => {
            options.reject(&[
                "--method",
                "--deadline",
                "--timing",
                "--delay",
                "--exceedance",
            ])?;
            load_failure_calibration(&args[1], &args[2], options.number("--alpha", 0.05)?)
        }
        "outcomes" => {
            options.reject(&["--alpha", "--method"])?;
            let timing = match options.value("--timing", "observed") {
                "observed" => {
                    options.reject(&["--delay", "--exceedance"])?;
                    Timing::Observed
                }
                "timely" => {
                    options.reject(&["--delay", "--exceedance"])?;
                    Timing::Timely
                }
                "queue" => Timing::QueueEnvelope {
                    max_delay: options.number("--delay", 0.0)?,
                    exceedance: options.number("--exceedance", 0.0)?,
                },
                "timely-queue" => {
                    options.reject(&["--exceedance"])?;
                    Timing::TimelyQueueEnvelope {
                        max_delay: options.number("--delay", 0.0)?,
                    }
                }
                _ => return Err(invalid("unknown timing mode")),
            };
            load_outcomes(
                &args[1],
                &args[2],
                options.number("--deadline", 180.0)?,
                timing,
            )
        }
        _ => Err(invalid("unknown input format")),
    }
}

fn report(study: &Study, policy: &Policy, include_failure: bool) {
    println!(
        "worst_cost={:.12}\nactive_columns={}",
        policy.worst_cost, policy.active_columns
    );
    if let (true, Some(failure)) = (include_failure, policy.worst_failure) {
        println!("worst_failure={failure:.12}");
    }
    let heading = if include_failure { "\tfailure" } else { "" };
    let failure_column = |failure: Option<f64>| match (include_failure, failure) {
        (true, Some(value)) => format!("\t{value:.9}"),
        (true, None) => "\t".to_owned(),
        (false, _) => String::new(),
    };
    println!("scenario\tacceptance\tjoint_defect\tcost\tlateness\trisk_score{heading}");
    for (scenario, m) in study.scenarios().iter().zip(&policy.scenarios) {
        println!(
            "{scenario}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{:.9}{}",
            m.acceptance,
            m.joint_defect,
            m.cost,
            m.lateness,
            m.risk_score,
            failure_column(m.failure)
        );
    }
    println!("group\tscenario\tacceptance\tjoint_defect\tcost\tlateness\trisk_score{heading}");
    for (group, rows) in study.groups().iter().zip(&policy.groups) {
        for (scenario, m) in study.scenarios().iter().zip(rows) {
            println!(
                "{group}\t{scenario}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{:.9}{}",
                m.acceptance,
                m.joint_defect,
                m.cost,
                m.lateness,
                m.risk_score,
                failure_column(m.failure)
            );
        }
    }
    println!("group\tcandidate\tweight");
    for (group, weights) in study.groups().iter().zip(&policy.weights) {
        for (candidate, &weight) in study.candidates().iter().zip(weights) {
            if weight > 0.0 {
                println!("{group}\t{candidate}\t{weight:.12}");
            }
        }
    }
}

fn selection(
    command: &str,
    args: &[String],
    options: &Options<'_>,
    scope: ServiceScope,
) -> Result<(), Error> {
    if command == "price" {
        options.reject(&["--routing", "--families"])?;
        if options.prune {
            return Err(invalid("--prune applies to cost, failure and cost-gap"));
        }
    } else {
        options.reject(&["--tolerance", "--seed"])?;
    }
    if command != "failure" {
        options.reject(&["--budget"])?;
    }
    if command == "cost-gap" && options.value("--input", "moments") != "failure-calibration" {
        return Err(invalid("cost-gap requires --input failure-calibration"));
    }
    let req = SelectionRequirements {
        max_failure: options.optional_number("--failure")?,
        completion: options.limit("--completion", 0.80)?,
        max_defect: options.limit("--epsilon", 0.10)?,
        max_lateness: options.limit("--lateness", 0.15)?,
        scope,
    };
    let mut study = study(args, options, req.max_defect)?;
    let restriction = match options.value("--routing", "mixed") {
        "mixed" => {
            options.reject(&["--families"])?;
            RoutingRestriction::Unrestricted
        }
        "family" => {
            let path = options
                .values
                .get("--families")
                .ok_or_else(|| invalid("family routing requires --families"))?;
            RoutingRestriction::OneFamily(load_families(path, study.candidates())?)
        }
        "fixed" => {
            options.reject(&["--families"])?;
            RoutingRestriction::OneConfiguration
        }
        _ => return Err(invalid("routing must be mixed, family or fixed")),
    };
    if let Some(excluded) = options.values.get("--exclude") {
        let mut eligible = vec![true; study.candidates().len()];
        for id in excluded.split(',') {
            let index = candidate_id(&study, id, "excluded")?;
            if !eligible[index] {
                return Err(invalid(format!("duplicate excluded candidate {id}")));
            }
            eligible[index] = false;
        }
        let n_groups = study.groups().len();
        study = study.with_eligibility(vec![eligible; n_groups])?;
    }
    let result = match command {
        "cost" => Some(study.solve_selection(req, &restriction, options.prune)?),
        "failure" => {
            let budget = options
                .optional_number("--budget")?
                .ok_or_else(|| invalid("failure requires --budget"))?;
            Some(study.minimise_failure(req, &restriction, budget, options.prune)?)
        }
        "cost-gap" => {
            let result = study.calibrate_selection(req, &restriction, options.prune)?;
            println!("optimistic_feasible={}", result.optimistic_cost.is_some());
            println!("conservative_feasible={}", result.conservative.is_some());
            if let Some(cost) = result.optimistic_cost {
                println!("optimistic_cost={cost:.12}");
            }
            if let Some(policy) = &result.conservative {
                println!("conservative_cost={:.12}", policy.worst_cost);
            }
            if let Some(gap) = result.cost_gap {
                println!("cost_gap={gap:.12}");
            }
            result.conservative
        }
        "price" => {
            let seed = seed_indices(&study, options)?;
            let result = study.column_generate_selection(
                req,
                &seed,
                options.number("--tolerance", 1e-10)?,
            )?;
            println!(
                "iterations={}\ngap_bound={:.12}",
                result.iterations, result.gap_bound
            );
            Some(result.policy)
        }
        _ => unreachable!(),
    };
    if let Some(policy) = result {
        report(&study, &policy, true);
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() || (args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h")) {
        print!("{HELP}");
        return Ok(());
    }
    let command = args[0].as_str();
    if args.len() < 3
        || !matches!(
            command,
            "cost" | "failure" | "cost-gap" | "risk" | "risk-one-lp" | "price"
        )
    {
        return Err(invalid("expected a command and two CSV paths; use --help"));
    }
    let options = Options::parse(&args[3..])?;
    let scope = match options.value("--scope", "scenario") {
        "scenario" => ServiceScope::Scenario,
        "group" => ServiceScope::Group,
        _ => return Err(invalid("scope must be scenario or group")),
    };
    if matches!(command, "failure" | "cost-gap")
        || (matches!(command, "cost" | "price") && options.uses_selection_options())
    {
        return selection(command, args, &options, scope);
    }
    options.reject(&["--failure", "--routing", "--families", "--exclude"])?;
    if options.value("--input", "moments") == "failure-calibration" {
        return Err(invalid(
            "failure calibration applies to cost, failure, cost-gap and price",
        ));
    }
    let req = Requirements {
        epsilon: options.number("--epsilon", 0.10)?,
        completion: options.number("--completion", 0.80)?,
        lateness: options.number("--lateness", 0.15)?,
        scope,
    };
    if options.prune && command != "cost" {
        return Err(invalid("--prune applies to cost, failure and cost-gap"));
    }
    let study = study(args, &options, Some(req.epsilon))?;
    let policy = match command {
        "cost" => {
            options.reject(&["--budget", "--tolerance", "--seed"])?;
            study.solve(req, options.prune)?
        }
        "risk" | "risk-one-lp" => {
            options.reject(&["--seed", "--epsilon"])?;
            let budget = options.number("--budget", f64::NAN)?;
            if command == "risk" {
                let solution =
                    study.minimise_risk(req, budget, options.number("--tolerance", 1e-7)?)?;
                println!(
                    "risk_lower={:.12}\nrisk_upper={:.12}",
                    solution.lower, solution.upper
                );
                solution.policy
            } else {
                options.reject(&["--tolerance"])?;
                let policy = study.minimise_risk_one_lp(req, budget)?;
                if let Some(risk) = policy.scenarios[0].defect_rate {
                    println!("risk={:.12}", risk.min(1.0));
                }
                policy
            }
        }
        "price" => {
            options.reject(&["--budget"])?;
            let seed = seed_indices(&study, &options)?;
            let result =
                study.column_generate(req, &seed, options.number("--tolerance", 1e-10)?)?;
            println!(
                "iterations={}\ngap_bound={:.12}",
                result.iterations, result.gap_bound
            );
            result.policy
        }
        _ => unreachable!(),
    };
    report(&study, &policy, false);
    Ok(())
}

fn main() -> ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(if error == Error::Infeasible { 2 } else { 1 })
        }
    }
}
