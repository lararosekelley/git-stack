mod common;

use common::TestRepo;

#[test]
fn credits_lists_inspirations_with_links() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["credits"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Graphite"))
        .stdout(predicates::str::contains("git-branchless"))
        .stdout(predicates::str::contains("https://graphite.dev"))
        .stdout(predicates::str::contains("https://github.com/jj-vcs/jj"));
}
