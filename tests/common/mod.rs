use std::path::Path;

use git2::{Repository, Signature, Time};

/// Fixed author timestamps used by [`create_test_repo`]. UTC, ascending so
/// integration tests can assert sort order without flake.
pub const ALICE_C1_EPOCH: i64 = 1_735_689_600; // 2025-01-01T00:00:00Z
pub const BOB_C2_EPOCH: i64 = 1_735_776_000; //   2025-01-02T00:00:00Z
pub const ALICE_C3_EPOCH: i64 = 1_735_862_400; // 2025-01-03T00:00:00Z

/// Create a test git repository with known commits from two authors.
///
/// Layout:
/// - Commit 1 (Alice, [`ALICE_C1_EPOCH`]): adds `main.rs` (3 lines) and `data.lock` (5 lines)
/// - Commit 2 (Bob,   [`BOB_C2_EPOCH`]):   modifies `main.rs` (+2 lines)
/// - Commit 3 (Alice, [`ALICE_C3_EPOCH`]): adds `README.md` (2 lines)
///
/// Author timestamps are fixed (not `Signature::now`) so JSON-shape tests
/// can pin specific dates.
pub fn create_test_repo(path: &Path) {
    let repo = Repository::init(path).unwrap();

    let alice_c1 = Signature::new(
        "Alice Smith",
        "alice@example.com",
        &Time::new(ALICE_C1_EPOCH, 0),
    )
    .unwrap();
    let bob = Signature::new("Bob Jones", "bob@example.com", &Time::new(BOB_C2_EPOCH, 0)).unwrap();
    let alice_c3 = Signature::new(
        "Alice Smith",
        "alice@example.com",
        &Time::new(ALICE_C3_EPOCH, 0),
    )
    .unwrap();
    let alice = &alice_c1;

    // Commit 1: Alice adds main.rs and data.lock.
    let blob_main = repo
        .blob(b"fn main() {\n    println!(\"hello\");\n}\n")
        .unwrap();
    let blob_lock = repo
        .blob(b"dep1=1.0\ndep2=2.0\ndep3=3.0\ndep4=4.0\ndep5=5.0\n")
        .unwrap();
    let mut tb = repo.treebuilder(None).unwrap();
    tb.insert("main.rs", blob_main, 0o100_644).unwrap();
    tb.insert("data.lock", blob_lock, 0o100_644).unwrap();
    let tree = repo.find_tree(tb.write().unwrap()).unwrap();
    let c1 = repo
        .commit(
            Some("HEAD"),
            alice,
            alice,
            "feat: initial setup",
            &tree,
            &[],
        )
        .unwrap();

    // Commit 2: Bob modifies main.rs (+2 lines).
    let blob_main2 = repo
        .blob(
            b"fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n    dbg!(42);\n}\n",
        )
        .unwrap();
    let mut tb2 = repo.treebuilder(None).unwrap();
    tb2.insert("main.rs", blob_main2, 0o100_644).unwrap();
    tb2.insert("data.lock", blob_lock, 0o100_644).unwrap();
    let tree2 = repo.find_tree(tb2.write().unwrap()).unwrap();
    let c1_commit = repo.find_commit(c1).unwrap();
    let c2 = repo
        .commit(
            Some("HEAD"),
            &bob,
            &bob,
            "feat: add debug output",
            &tree2,
            &[&c1_commit],
        )
        .unwrap();

    // Commit 3: Alice adds README.md.
    let blob_readme = repo.blob(b"# Project\nA test project.\n").unwrap();
    let mut tb3 = repo.treebuilder(None).unwrap();
    tb3.insert("main.rs", blob_main2, 0o100_644).unwrap();
    tb3.insert("data.lock", blob_lock, 0o100_644).unwrap();
    tb3.insert("README.md", blob_readme, 0o100_644).unwrap();
    let tree3 = repo.find_tree(tb3.write().unwrap()).unwrap();
    let c2_commit = repo.find_commit(c2).unwrap();
    repo.commit(
        Some("HEAD"),
        &alice_c3,
        &alice_c3,
        "docs: add README",
        &tree3,
        &[&c2_commit],
    )
    .unwrap();
}

/// Create a repo with a single Alice commit using a "wrong" author email,
/// used by the `--mailmap-file` integration tests.
///
/// Returns the path of the created `main.rs` (caller may need it).
pub fn create_repo_with_unmapped_alice(path: &Path) {
    let repo = Repository::init(path).unwrap();
    let alice_wrong = Signature::new(
        "Alice Old",
        "alice-old@example.com",
        &Time::new(ALICE_C1_EPOCH, 0),
    )
    .unwrap();

    let blob = repo.blob(b"hello\n").unwrap();
    let mut tb = repo.treebuilder(None).unwrap();
    tb.insert("main.rs", blob, 0o100_644).unwrap();
    let tree = repo.find_tree(tb.write().unwrap()).unwrap();
    repo.commit(
        Some("HEAD"),
        &alice_wrong,
        &alice_wrong,
        "first",
        &tree,
        &[],
    )
    .unwrap();
}
