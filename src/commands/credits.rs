use anyhow::Result;

use crate::commands::Run;
use crate::style;

/// The tools that charted stacked work before git-stk, each with the homepage
/// or repo to go read for yourself. Ordered loosely by how directly they shaped
/// the workflow here.
const INSPIRATIONS: &[(&str, &str, &str)] = &[
    (
        "Graphite",
        "A faster, more intuitive Git interface",
        "https://graphite.dev",
    ),
    (
        "spr",
        "Stacked Pull Requests on GitHub",
        "https://github.com/ejoffe/spr",
    ),
    (
        "ghstack",
        "Submit stacked diffs to GitHub on the command line",
        "https://github.com/ezyang/ghstack",
    ),
    (
        "git-branchless",
        "High-velocity, monorepo-scale workflow for Git",
        "https://github.com/arxanas/git-branchless",
    ),
    (
        "git-town",
        "Git branches made easy",
        "https://www.git-town.com",
    ),
    (
        "Sapling",
        "A Scalable, User-Friendly Source Control System",
        "https://sapling-scm.com",
    ),
    (
        "Jujutsu (jj)",
        "A Git-compatible VCS that is both simple and powerful",
        "https://github.com/jj-vcs/jj",
    ),
];

/// Show attribution for the tools that inspired git-stk.
#[derive(Debug, clap::Args)]
pub struct Credits;

impl Run for Credits {
    fn run(self) -> Result<()> {
        // Pad names to the widest so the blurbs line up in a column.
        let width = INSPIRATIONS
            .iter()
            .map(|(name, _, _)| name.chars().count())
            .max()
            .unwrap_or(0);

        anstream::println!(
            "git-stk could not have been made without the tools that explored stacked branch workflows before it:\n"
        );
        for (name, blurb, url) in INSPIRATIONS {
            // Pad the plain name to the column width, then paint just the name,
            // so the ANSI codes never throw off the alignment.
            let pad = " ".repeat(width - name.chars().count());
            anstream::println!("  {}{pad}  {blurb}", style::paint(style::BRANCH, name));
            // The URL sits indented under its blurb.
            let indent = " ".repeat(width + 2);
            anstream::println!("  {indent}{}", style::paint(style::DIM, url));
        }
        anstream::println!("{}", style::paint(style::DIM, "\nBuilt by Lara Kelley (@lararosekelley)"));
        Ok(())
    }
}
