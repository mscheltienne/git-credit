use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::create_test_repo;

// ---------------------------------------------------------------------------
// Basic flags
// ---------------------------------------------------------------------------

#[test]
fn help_flag() {
    Command::cargo_bin("git-credit")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("squash merges"));
}

#[test]
fn version_flag() {
    Command::cargo_bin("git-credit")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

// ---------------------------------------------------------------------------
// Running against a test repo
// ---------------------------------------------------------------------------

#[test]
fn table_output_on_test_repo() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    Command::cargo_bin("git-credit")
        .unwrap()
        .args(["--repo", dir.path().to_str().unwrap(), "--no-github"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice"))
        .stdout(predicate::str::contains("Bob"))
        .stdout(predicate::str::contains("commits walked"));
}

#[test]
fn json_output_on_test_repo() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = Command::cargo_bin("git-credit")
        .unwrap()
        .args([
            "--repo",
            dir.path().to_str().unwrap(),
            "--no-github",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["authors"].is_array());
    assert!(json["total_commits_walked"].as_u64().unwrap() >= 2);
}

#[test]
fn exclude_filters_files() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    // Without exclude: stats include data.lock.
    let without = Command::cargo_bin("git-credit")
        .unwrap()
        .args([
            "--repo",
            dir.path().to_str().unwrap(),
            "--no-github",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let without_json: serde_json::Value = serde_json::from_slice(&without.stdout).unwrap();

    // With exclude: stats should be smaller.
    let with = Command::cargo_bin("git-credit")
        .unwrap()
        .args([
            "--repo",
            dir.path().to_str().unwrap(),
            "--no-github",
            "--format",
            "json",
            "--exclude",
            "data.lock",
        ])
        .output()
        .unwrap();
    let with_json: serde_json::Value = serde_json::from_slice(&with.stdout).unwrap();

    let total_without: u64 = without_json["authors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["additions"].as_u64().unwrap())
        .sum();
    let total_with: u64 = with_json["authors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["additions"].as_u64().unwrap())
        .sum();

    assert!(total_with < total_without);
}

#[test]
fn no_github_flag_works_without_network() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    Command::cargo_bin("git-credit")
        .unwrap()
        .args(["--repo", dir.path().to_str().unwrap(), "--no-github"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn invalid_repo_path_errors() {
    Command::cargo_bin("git-credit")
        .unwrap()
        .args(["--repo", "/nonexistent/path", "--no-github"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not open git repository"));
}

#[test]
fn invalid_since_date_errors() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    Command::cargo_bin("git-credit")
        .unwrap()
        .args([
            "--repo",
            dir.path().to_str().unwrap(),
            "--no-github",
            "--since",
            "not-a-date",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid --since date"));
}
