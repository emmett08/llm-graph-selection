//! Execution checks against the paper's frozen synthetic fixtures and exact examples.

use llm_workflow_optimiser::calibration::CalibrationMethod;
use llm_workflow_optimiser::io::{load_calibration, load_moments, load_outcomes, Timing};
use llm_workflow_optimiser::{Error, Requirements, ServiceScope};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn service() -> Requirements {
    Requirements {
        epsilon: 0.10,
        completion: 0.80,
        lateness: 0.15,
        scope: ServiceScope::Scenario,
    }
}

fn appendix_service() -> Requirements {
    Requirements {
        epsilon: 0.10,
        completion: 0.60,
        lateness: 0.08,
        scope: ServiceScope::Scenario,
    }
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.12}, got {actual:.12}"
    );
}

#[test]
fn aggregate_optimum_survives_dominance_pruning() {
    let study = load_moments(
        fixture("population-moments.csv"),
        fixture("scenario-weights.csv"),
    )
    .unwrap();
    let direct = study.solve(service(), false).unwrap();
    let pruned = study.solve(service(), true).unwrap();
    assert_eq!(direct.active_columns, 110);
    assert_eq!(pruned.active_columns, 20);
    close(direct.worst_cost, 0.038_798_338_324_895, 1e-8);
    close(pruned.worst_cost, direct.worst_cost, 1e-8);
    for (group, row) in pruned.weights.iter().enumerate() {
        close(row.iter().sum(), 1.0, 1e-9);
        assert!(row.iter().all(|&weight| weight >= -1e-10));
        for draw in [0.0, 0.5, 1.0 - f64::EPSILON] {
            assert!(row[pruned.select(group, draw).unwrap()] > 0.0);
        }
        assert!(pruned.select(group, 1.0).is_err());
    }
    let positive = pruned
        .weights
        .iter()
        .flatten()
        .filter(|&&w| w > 1e-8)
        .count();
    assert!(positive < study.groups().len() + 4 * study.scenarios().len());
    let migration = study
        .groups()
        .iter()
        .position(|id| id == "migration")
        .unwrap();
    let adverse = study
        .scenarios()
        .iter()
        .position(|id| id == "adverse")
        .unwrap();
    close(
        direct.groups[migration][adverse].acceptance,
        0.679_926_838_731_465,
        1e-7,
    );
}

#[test]
fn every_group_can_meet_service_limits_for_a_higher_objective() {
    let study = load_moments(
        fixture("population-moments.csv"),
        fixture("scenario-weights.csv"),
    )
    .unwrap();
    let requirements = Requirements {
        scope: ServiceScope::Group,
        ..service()
    };
    let policy = study.solve(requirements, true).unwrap();
    close(policy.worst_cost, 0.042_376_800_201_543, 1e-8);
    for group in &policy.groups {
        for outcome in group {
            assert!(outcome.acceptance + 1e-8 >= 0.80);
            assert!(outcome.defect_rate.unwrap() <= 0.10 + 1e-8);
            assert!(outcome.lateness <= 0.15 + 1e-8);
        }
    }
}

#[test]
fn paired_calibration_reproduces_the_frozen_sample() {
    for (method, expected, per_group_expected) in [
        (
            CalibrationMethod::SeparateHoeffding,
            0.046_827_162_459_517,
            0.052_194_709_463_257,
        ),
        (
            CalibrationMethod::PairedHoeffding { epsilon: 0.10 },
            0.045_977_417_449_850,
            0.051_528_297_572_118,
        ),
        (
            CalibrationMethod::PairedEmpiricalBernstein { epsilon: 0.10 },
            0.043_380_378_642_002,
            0.049_169_544_874_142,
        ),
    ] {
        let study = load_calibration(
            fixture("calibration-tallies.csv"),
            fixture("scenario-weights.csv"),
            0.05,
            method,
        )
        .unwrap();
        close(
            study.solve(service(), true).unwrap().worst_cost,
            expected,
            1e-8,
        );
        let requirements = Requirements {
            scope: ServiceScope::Group,
            ..service()
        };
        close(
            study.solve(requirements, true).unwrap().worst_cost,
            per_group_expected,
            1e-8,
        );
    }
}

#[test]
fn column_generation_recovers_the_full_pool_optimum() {
    let study = load_moments(
        fixture("population-moments.csv"),
        fixture("scenario-weights.csv"),
    )
    .unwrap();
    let seed: Vec<_> = study
        .candidates()
        .iter()
        .enumerate()
        .filter(|(_, id)| {
            id.contains("__single__")
                || *id == "reject"
                || *id == "gemini-4-hydrogen__serial__high__8192"
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(seed.len(), 20);
    let solution = study.column_generate(service(), &seed, 1e-8).unwrap();
    close(solution.policy.worst_cost, 0.038_798_338_324_895, 1e-8);
    assert!((0.0..=2e-8).contains(&solution.gap_bound));
    assert!(solution.iterations > 0 && solution.retained.len() <= study.candidates().len());
}

#[test]
fn appendix_matches_cost_and_fixed_budget_certificates() {
    let study = load_moments(
        fixture("appendix-moments.csv"),
        fixture("appendix-weights.csv"),
    )
    .unwrap();
    let requirements = appendix_service();
    let direct = study.solve(requirements, false).unwrap();
    close(direct.worst_cost, 172.0 / 135.0, 1e-9);
    let seeded = study
        .column_generate(requirements, &[0, 1, 2], 1e-8)
        .unwrap();
    close(seeded.policy.worst_cost, direct.worst_cost, 1e-9);
    assert!(seeded.retained.contains(&3));
    assert!((0.0..=1e-8).contains(&seeded.gap_bound));

    let adverse = load_moments(
        fixture("appendix-adverse-moments.csv"),
        fixture("appendix-adverse-weights.csv"),
    )
    .unwrap();
    let one_lp = adverse.minimise_risk_one_lp(requirements, 1.3).unwrap();
    let expected = 123.0 / 1_300.0;
    close(one_lp.scenarios[0].defect_rate.unwrap(), expected, 1e-9);
    let bracket = study.minimise_risk(requirements, 1.3, 1e-7).unwrap();
    assert!(bracket.lower <= expected + 1e-8 && bracket.upper >= expected - 1e-8);
    assert!(bracket.upper - bracket.lower <= 1e-7 + 1e-10);
    close(
        bracket.policy.scenarios[1].defect_rate.unwrap(),
        expected,
        1e-7,
    );
}

#[test]
fn timed_outcomes_recover_the_observed_and_timely_objectives() {
    let observed = load_outcomes(
        fixture("finite-outcomes.csv"),
        fixture("scenario-weights.csv"),
        180.0,
        Timing::Observed,
    )
    .unwrap();
    close(
        observed.solve(service(), true).unwrap().worst_cost,
        0.038_798_338_324_895,
        1e-8,
    );

    let timely = load_outcomes(
        fixture("finite-outcomes.csv"),
        fixture("scenario-weights.csv"),
        180.0,
        Timing::Timely,
    )
    .unwrap();
    close(
        timely.solve(service(), true).unwrap().worst_cost,
        0.047_703_750_935_471,
        1e-8,
    );
}

#[test]
fn added_queue_delay_can_change_the_routing_or_remove_feasibility() {
    let build_study = |max_delay| {
        load_outcomes(
            fixture("finite-outcomes.csv"),
            fixture("scenario-weights.csv"),
            180.0,
            Timing::QueueEnvelope {
                max_delay,
                exceedance: 0.0,
            },
        )
        .unwrap()
    };
    for (delay, expected) in [
        (0.0, 0.038_798_338_324_895),
        (5.0, 0.038_896_653_453_198),
        (30.0, 0.040_702_696_247_474),
    ] {
        close(
            build_study(delay)
                .solve(service(), true)
                .unwrap()
                .worst_cost,
            expected,
            1e-8,
        );
    }
    assert!(matches!(
        build_study(50.0).solve(service(), true),
        Err(Error::Infeasible)
    ));
}

#[test]
fn timely_queue_envelope_uses_both_ends_of_the_delay_interval() {
    let study = load_outcomes(
        fixture("finite-outcomes.csv"),
        fixture("scenario-weights.csv"),
        180.0,
        Timing::TimelyQueueEnvelope { max_delay: 5.0 },
    )
    .unwrap();
    close(
        study.solve(service(), true).unwrap().worst_cost,
        0.050_892_747_563_523,
        1e-8,
    );
}
