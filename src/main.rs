use llm_workflow_optimiser::{
    calibration::CalibrationMethod,
    io::{load_calibration, load_moments, load_outcomes, Timing},
    Error, Policy, Requirements, ServiceScope, Study,
};
use std::collections::HashMap;
use std::process::ExitCode;

const HELP: &str =
    "llm-workflow-optimiser <cost|risk|risk-one-lp|price> <input.csv> <scenarios.csv> [options]

Commands
  cost         Minimise worst-scenario cost at a fixed defect limit
  risk         Minimise worst-scenario defect risk at a fixed budget
  risk-one-lp  Minimise defect risk with one scenario using Charnes–Cooper
  price        Price the finite candidate pool from a feasible seed

Service options (defaults in parentheses)
  --epsilon <probability>       Accepted-output defect limit (0.10)
  --completion <probability>    Completion floor (0.80)
  --lateness <probability>      Lateness ceiling (0.15)
  --scope <scenario|group>      Apply limits to each scenario or group (scenario)
  --prune                     Screen dominated columns (cost only)
  --budget <cost>              Worst-scenario cost ceiling (risk commands)
  --tolerance <number>         Bisection width (1e-7) or pricing tolerance in cost units (1e-10)
  --seed <id,id,...>            Feasible candidate seed (price only)

Input options
  --input <moments|calibration|outcomes>  Input format (moments)
  --alpha <probability>         Family error probability (0.05)
  --method <separate|paired|bernstein>    Calibration method (separate)
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
        self.values.get(name).map_or(Ok(default), |value| {
            value
                .parse()
                .map_err(|_| invalid(format!("{name} needs a number")))
        })
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

fn study(args: &[String], options: &Options<'_>, req: Requirements) -> Result<Study, Error> {
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
                    epsilon: req.epsilon,
                },
                "bernstein" => CalibrationMethod::PairedEmpiricalBernstein {
                    epsilon: req.epsilon,
                },
                _ => return Err(invalid("unknown calibration method")),
            };
            load_calibration(&args[1], &args[2], options.number("--alpha", 0.05)?, method)
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

fn report(study: &Study, policy: &Policy) {
    println!(
        "worst_cost={:.12}\nactive_columns={}",
        policy.worst_cost, policy.active_columns
    );
    println!("scenario\tacceptance\tjoint_defect\tcost\tlateness\trisk_score");
    for (scenario, m) in study.scenarios().iter().zip(&policy.scenarios) {
        println!(
            "{scenario}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{:.9}",
            m.acceptance, m.joint_defect, m.cost, m.lateness, m.risk_score
        );
    }
    println!("group\tscenario\tacceptance\tjoint_defect\tcost\tlateness\trisk_score");
    for (group, rows) in study.groups().iter().zip(&policy.groups) {
        for (scenario, m) in study.scenarios().iter().zip(rows) {
            println!(
                "{group}\t{scenario}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{:.9}",
                m.acceptance, m.joint_defect, m.cost, m.lateness, m.risk_score
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

fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() || (args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h")) {
        print!("{HELP}");
        return Ok(());
    }
    let command = args[0].as_str();
    if args.len() < 3 || !matches!(command, "cost" | "risk" | "risk-one-lp" | "price") {
        return Err(invalid("expected a command and two CSV paths; use --help"));
    }
    let options = Options::parse(&args[3..])?;
    let req = Requirements {
        epsilon: options.number("--epsilon", 0.10)?,
        completion: options.number("--completion", 0.80)?,
        lateness: options.number("--lateness", 0.15)?,
        scope: match options.value("--scope", "scenario") {
            "scenario" => ServiceScope::Scenario,
            "group" => ServiceScope::Group,
            _ => return Err(invalid("scope must be scenario or group")),
        },
    };
    if options.prune && command != "cost" {
        return Err(invalid("--prune applies only to cost"));
    }
    let study = study(args, &options, req)?;
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
            let names = options
                .values
                .get("--seed")
                .ok_or_else(|| invalid("price requires --seed"))?;
            let seed = names
                .split(',')
                .map(|id| {
                    study
                        .candidates()
                        .iter()
                        .position(|candidate| candidate == id)
                        .ok_or_else(|| invalid(format!("unknown seed candidate {id}")))
                })
                .collect::<Result<Vec<_>, _>>()?;
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
    report(&study, &policy);
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
