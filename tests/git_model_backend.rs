#![allow(dead_code)]

#[path = "../src/models.rs"]
mod models;

#[path = "../src/git_model.rs"]
mod git_model;

use git_model::{StatusSectionKind, classify_status_xy, summarize_porcelain_status};

#[test]
fn classifies_porcelain_xy_pairs() {
    assert_eq!(classify_status_xy(" M"), vec![StatusSectionKind::Unstaged]);
    assert_eq!(classify_status_xy("A "), vec![StatusSectionKind::Staged]);
    assert_eq!(classify_status_xy("??"), vec![StatusSectionKind::Untracked]);
}

#[test]
fn summarizes_grouped_status_counts() {
    let status = concat!(
        " M src/main.rs\n",
        "A  src/lib.rs\n",
        "MM src/config.toml\n",
        "?? Cargo.lock\n",
    );

    let summaries = summarize_porcelain_status(status);
    assert_eq!(summaries.len(), 3);
    assert_eq!(summaries[0].kind, StatusSectionKind::Staged);
    assert_eq!(summaries[0].count, 2);
    assert_eq!(summaries[1].kind, StatusSectionKind::Unstaged);
    assert_eq!(summaries[1].count, 1);
    assert_eq!(summaries[2].kind, StatusSectionKind::Untracked);
    assert_eq!(summaries[2].count, 1);
}
