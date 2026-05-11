use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use regex::Regex;

mod common;
use common::{
    ALICE_C1_EPOCH, ALICE_C3_EPOCH, BOB_C2_EPOCH, create_repo_with_unmapped_alice, create_test_repo,
};

// Build-time sanity: the fixture ordering documented in `create_test_repo`
// matches what the integration tests rely on.
const _: () = {
    assert!(ALICE_C1_EPOCH < BOB_C2_EPOCH);
    assert!(BOB_C2_EPOCH < ALICE_C3_EPOCH);
};

fn sum_additions(json: &serde_json::Value) -> u64 {
    json["commits"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["attributions"].as_array().unwrap())
        .map(|a| a["additions"].as_u64().unwrap())
        .sum()
}

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
fn json_output_has_per_commit_shape() {
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

    assert!(output.status.success(), "git-credit failed: {output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let commits = json["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 3);

    for commit in commits {
        assert!(commit["sha"].is_string());
        assert!(commit["author_date"].is_string());
        assert!(commit["is_squash_pr"].is_boolean());
        let attributions = commit["attributions"].as_array().unwrap();
        assert!(!attributions.is_empty());
        for a in attributions {
            assert!(a["name"].is_string());
            assert!(a["email"].is_string());
            assert!(a["additions"].is_u64());
            assert!(a["deletions"].is_u64());
            assert!(a["is_pr_author"].is_boolean());
        }
    }

    assert_eq!(json["summary"]["total_commits_walked"].as_u64().unwrap(), 3);
    assert_eq!(
        json["summary"]["squash_merges_expanded"].as_u64().unwrap(),
        0
    );
}

#[test]
fn author_date_format_is_iso8601_utc() {
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
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let iso_re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$").unwrap();
    for commit in json["commits"].as_array().unwrap() {
        let date = commit["author_date"].as_str().unwrap();
        assert!(iso_re.is_match(date), "bad author_date: {date}");
    }
}

#[test]
fn commits_sorted_by_author_date_ascending() {
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
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let commits = json["commits"].as_array().unwrap();

    // Fixed timestamps from create_test_repo, ascending.
    let dates: Vec<&str> = commits
        .iter()
        .map(|c| c["author_date"].as_str().unwrap())
        .collect();
    assert_eq!(dates[0], "2025-01-01T00:00:00Z"); // ALICE_C1_EPOCH
    assert_eq!(dates[1], "2025-01-02T00:00:00Z"); // BOB_C2_EPOCH
    assert_eq!(dates[2], "2025-01-03T00:00:00Z"); // ALICE_C3_EPOCH
}

#[test]
fn exclude_filters_files() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

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

    assert!(sum_additions(&with_json) < sum_additions(&without_json));
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
// --mailmap-file
// ---------------------------------------------------------------------------

#[test]
fn mailmap_file_overrides_repo_mailmap() {
    let dir = tempfile::tempdir().unwrap();
    create_repo_with_unmapped_alice(dir.path());

    // In-repo mailmap maps the wrong email to a "wrong" target.
    fs::write(
        dir.path().join(".mailmap"),
        "Alice Wrong <alice-wrong@example.com> <alice-old@example.com>\n",
    )
    .unwrap();

    // External mailmap maps the wrong email to the canonical identity.
    let external_dir = tempfile::tempdir().unwrap();
    let external_path = external_dir.path().join(".mailmap");
    fs::write(
        &external_path,
        "Alice Smith <alice@example.com> <alice-old@example.com>\n",
    )
    .unwrap();

    let output = Command::cargo_bin("git-credit")
        .unwrap()
        .args([
            "--repo",
            dir.path().to_str().unwrap(),
            "--no-github",
            "--format",
            "json",
            "--mailmap-file",
            external_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "git-credit failed: {output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let attribution = &json["commits"][0]["attributions"][0];
    assert_eq!(attribution["email"], "alice@example.com");
    assert_eq!(attribution["name"], "Alice Smith");
}

#[test]
fn mailmap_file_missing_errors() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    Command::cargo_bin("git-credit")
        .unwrap()
        .args([
            "--repo",
            dir.path().to_str().unwrap(),
            "--no-github",
            "--mailmap-file",
            "/definitely/does/not/exist/.mailmap",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not read mailmap file"));
}

#[test]
fn mailmap_file_invalid_errors() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    // Embedded NUL byte → CString::new fails inside git2::Mailmap::from_buffer.
    let bad_dir = tempfile::tempdir().unwrap();
    let bad_path = bad_dir.path().join("bad.mailmap");
    fs::write(&bad_path, b"Alice <a@x.com>\0invalid\n").unwrap();

    Command::cargo_bin("git-credit")
        .unwrap()
        .args([
            "--repo",
            dir.path().to_str().unwrap(),
            "--no-github",
            "--mailmap-file",
            bad_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid mailmap file"));
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
