use std::path::Path;

use git2::{Repository, Signature};

/// Create a test git repository with known commits from two authors.
///
/// Layout:
/// - Commit 1 (Alice): adds `main.rs` (3 lines) and `data.lock` (5 lines)
/// - Commit 2 (Bob): modifies `main.rs` (+2 lines)
/// - Commit 3 (Alice): adds `README.md` (2 lines)
pub fn create_test_repo(path: &Path) {
    let repo = Repository::init(path).unwrap();

    let alice = Signature::now("Alice Smith", "alice@example.com").unwrap();
    let bob = Signature::now("Bob Jones", "bob@example.com").unwrap();

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
            &alice,
            &alice,
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
        &alice,
        &alice,
        "docs: add README",
        &tree3,
        &[&c2_commit],
    )
    .unwrap();
}
