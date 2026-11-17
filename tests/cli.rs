use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_llm-workflow-optimiser"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("run optimiser")
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 report")
}

fn metric(report: &str, key: &str) -> f64 {
    report
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(name, value)| (name == key).then_some(value))
        .unwrap_or_else(|| panic!("missing metric {key} in report:\n{report}"))
        .parse()
        .expect("numeric metric")
}

#[test]
fn cost_at_thirty_percent_matches_the_four_paper_comparisons() {
    let cases: &[(&[&str], f64)] = &[
        (&[], 0.04454362),
        (&["--exclude", "reject"], 0.0483133),
        (
            &[
                "--exclude",
                "reject",
                "--routing",
                "family",
                "--families",
                "fixtures/population-moments.csv",
            ],
            0.05760394,
        ),
        (&["--exclude", "reject", "--routing", "fixed"], 0.07677),
    ];
    for &(options, cost) in cases {
        let mut args = vec![
            "cost",
            "fixtures/finite-outcomes.csv",
            "fixtures/scenario-weights.csv",
            "--input",
            "outcomes",
            "--failure",
            "0.30",
            "--prune",
        ];
        args.extend_from_slice(options);
        let report = stdout(run(&args));
        assert!((metric(&report, "worst_cost") - cost).abs() < 1e-7);
        assert!(metric(&report, "worst_failure") <= 0.30 + 1e-8);
        assert!(report.contains("risk_score\tfailure\n"));
    }
}

#[test]
fn failure_under_a_budget_and_legacy_cost_keep_their_respective_objectives() {
    let legacy = stdout(run(&[
        "cost",
        "fixtures/population-moments.csv",
        "fixtures/scenario-weights.csv",
        "--prune",
    ]));
    assert!(legacy.starts_with("worst_cost=0.038798338325\nactive_columns=20\n"));
    assert!(!legacy.contains("worst_failure="));

    let result = stdout(run(&[
        "failure",
        "fixtures/finite-outcomes.csv",
        "fixtures/scenario-weights.csv",
        "--input",
        "outcomes",
        "--budget",
        "0.04454363",
    ]));
    assert!(metric(&result, "worst_cost") <= 0.04454363 + 1e-8);
    assert!((metric(&result, "worst_failure") - 0.30).abs() < 1e-6);
}

#[test]
fn immediate_rejection_counts_as_failure_with_auxiliary_limits_omitted() {
    let result = stdout(run(&[
        "cost",
        "fixtures/finite-outcomes.csv",
        "fixtures/scenario-weights.csv",
        "--input",
        "outcomes",
        "--failure",
        "1",
        "--epsilon",
        "none",
        "--completion",
        "none",
        "--lateness",
        "none",
    ]));
    assert_eq!(metric(&result, "worst_cost"), 0.0);
    assert_eq!(metric(&result, "worst_failure"), 1.0);
}

#[test]
fn pricing_can_start_from_an_infeasible_seed_with_a_failure_limit() {
    let report = stdout(run(&[
        "price",
        "fixtures/finite-outcomes.csv",
        "fixtures/scenario-weights.csv",
        "--input",
        "outcomes",
        "--failure",
        "0.30",
        "--seed",
        "gpt-6.1-sol__single__low__4096",
    ]));
    assert!((metric(&report, "worst_cost") - 0.04454362).abs() < 1e-7);
    assert!(metric(&report, "worst_failure") <= 0.30 + 1e-8);
    assert!(metric(&report, "gap_bound") <= 1e-8);
}

#[test]
fn two_sided_calibration_reports_the_gap_and_inconclusive_feasibility() {
    let solve = |failure| {
        stdout(run(&[
            "cost-gap",
            "fixtures/calibration-tallies.csv",
            "fixtures/scenario-weights.csv",
            "--input",
            "failure-calibration",
            "--failure",
            failure,
        ]))
    };
    let feasible = solve("0.30");
    assert!(feasible.contains("conservative_feasible=true\n"));
    assert!((metric(&feasible, "optimistic_cost") - 0.03722412).abs() < 1e-7);
    assert!((metric(&feasible, "conservative_cost") - 0.05581493).abs() < 1e-7);
    assert!((metric(&feasible, "cost_gap") - 0.01859081).abs() < 1e-7);

    let inconclusive = solve("0.19");
    assert!(inconclusive.contains("optimistic_feasible=true\n"));
    assert!(inconclusive.contains("conservative_feasible=false\n"));
    assert!(!inconclusive.contains("worst_cost="));
}
