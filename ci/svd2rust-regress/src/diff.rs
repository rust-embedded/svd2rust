use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::github;
use crate::Opts;

#[derive(clap::Parser, Debug)]
#[clap(name = "diff")]
pub struct Diffing {
    #[clap(long)]
    pub base: Option<String>,

    #[clap(long)]
    pub head: Option<String>,

    /// Enable formatting with `rustfmt`
    #[clap(short = 'f', long)]
    pub format: bool,

    #[clap(subcommand)]
    pub sub: Option<DiffingSub>,

    #[clap(long)]
    pub chip: Vec<String>,
}

#[derive(clap::Parser, Debug)]
pub enum DiffingSub {
    Pr,
}

impl Diffing {
    pub fn run(&self, opts: &Opts, rustfmt_bin_path: Option<&Path>) -> Result<(), anyhow::Error> {
        let (head, base) = self
            .svd2rust_setup(opts)
            .with_context(|| "couldn't setup svd2rust")?;
        let tests = crate::tests::tests(Some(opts)).with_context(|| "no tests found")?;

        tests[0]
            .setup_case(
                &opts.output_dir.join("head"),
                &head,
                rustfmt_bin_path,
                false,
            )
            .with_context(|| "couldn't create head")?;
        tests[0]
            .setup_case(
                &opts.output_dir.join("base"),
                &base,
                rustfmt_bin_path,
                false,
            )
            .with_context(|| "couldn't create base")?;
        Ok(())
    }

    pub fn svd2rust_setup(&self, opts: &Opts) -> Result<(PathBuf, PathBuf), anyhow::Error> {
        let base = match self
            .base
            .as_deref()
            .and_then(|s| s.strip_prefix('@'))
            .and_then(|s| s.split(' ').next())
        {
            reference @ None | reference @ Some("" | "master") => {
                github::get_release_binary_artifact(reference.unwrap_or("master"), &opts.output_dir)
                    .with_context(|| "couldn't get svd2rust latest unreleased artifact")?
            }
            Some("pr") => {
                let (number, sha) =
                    github::get_current_pr().with_context(|| "couldn't get current pr")?;
                github::get_pr_binary_artifact(number, &sha, &opts.output_dir)
                    .with_context(|| "couldn't get pr artifact")?
            }
            Some(reference) => github::get_release_binary_artifact(reference, &opts.output_dir)
                .with_context(|| format!("could not get svd2rust for {reference}"))?,
        };

        let head = match self
            .head
            .as_deref()
            .and_then(|s| s.strip_prefix('@'))
            .and_then(|s| s.split(' ').next())
        {
            None | Some("" | "pr") => {
                let (number, sha) =
                    github::get_current_pr().with_context(|| "couldn't get current pr")?;
                github::get_pr_binary_artifact(number, &sha, &opts.output_dir)
                    .with_context(|| "couldn't get pr artifact")?
            }
            Some(reference) => github::get_release_binary_artifact(reference, &opts.output_dir)
                .with_context(|| format!("could not get svd2rust for {reference}"))?,
        };

        Ok((base, head))
    }
}

#[cfg(test)]
#[test]
pub fn diffing_cli_works() {
    use clap::Parser;

    Diffing::parse_from(["diff", "pr"]);
    Diffing::parse_from(["diff", "--base", "", "--head", "\"--atomics\""]);
    Diffing::parse_from(["diff", "--base", "\"@master\"", "--head", "\"@pr\""]);
    Diffing::parse_from([
        "diff",
        "--base",
        "\"@master\"",
        "--head",
        "\"@pr\"",
        "--chip",
        "STM32F401",
    ]);
    Diffing::parse_from([
        "diff",
        "--base",
        "\"@master\"",
        "--head",
        "\"@pr --atomics\"",
    ]);
    Diffing::parse_from(["diff", "--head", "\"--atomics\""]);
}
