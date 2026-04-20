use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn prints_hello_world() {
    Command::cargo_bin("git-credit")
        .unwrap()
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello, world!"));
}

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
