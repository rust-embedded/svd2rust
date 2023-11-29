use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::github;
use crate::Opts;

#[derive(clap::Parser, Debug)]
#[clap(name = "diff")]
pub struct Diffing {
    /// The base version of svd2rust to use and the command input, defaults to latest master build
    ///
    /// Change the base version by starting with `@` followed by the source.
    ///
    /// supports `@pr` for current pr, `@master` for latest master build, or a version tag like `@v0.30.0`
    #[clap(long)]
    pub base: Option<String>,

    #[clap(long)]
    pub head: Option<String>,

    /// Enable formatting with `rustfmt`
    #[clap(short = 'f', long)]
    pub format: bool,

    /// Enable splitting `lib.rs` with `form`
    #[clap(long)]
    pub form_lib: bool,

    #[clap(subcommand)]
    pub sub: Option<DiffingSub>,

    #[clap(long)]
    pub chip: Vec<String>,
}

#[derive(clap::Parser, Debug)]
pub enum DiffingSub {
    Pr,
}

type Source<'s> = Option<&'s str>;
type Command<'s> = Option<&'s str>;

impl Diffing {
    pub fn run(
        &self,
        opts: &Opts,
        rustfmt_bin_path: Option<&Path>,
        form_bin_path: Option<&Path>,
    ) -> Result<(), anyhow::Error> {
        let [head, base] = self
            .make_cases(opts, rustfmt_bin_path, form_bin_path)
            .with_context(|| "couldn't setup svd2rust")?;
        std::process::Command::new("git")
            .args(["--no-pager", "diff", "--no-index", "--minimal"])
            .args([&*head.0, &*base.0])
            .status()
            .with_context(|| "couldn't run git diff")
            .map(|_| ())
    }

    pub fn make_cases(
        &self,
        opts: &Opts,
        rustfmt_bin_path: Option<&Path>,
        form_bin_path: Option<&Path>,
    ) -> Result<[(PathBuf, Vec<PathBuf>); 2], anyhow::Error> {
        let [(head_bin, head_cmd), (base_bin, base_cmd)] = self
            .svd2rust_setup(opts)
            .with_context(|| "couldn't setup svd2rust")?;
        let tests = crate::tests::tests(Some(opts)).with_context(|| "no tests found")?;

        let head = tests[0]
            .setup_case(
                &opts.output_dir.join("head"),
                &head_bin,
                rustfmt_bin_path,
                form_bin_path,
                head_cmd,
            )
            .with_context(|| "couldn't create head")?;
        let base = tests[0]
            .setup_case(
                &opts.output_dir.join("base"),
                &base_bin,
                rustfmt_bin_path,
                form_bin_path,
                base_cmd,
            )
            .with_context(|| "couldn't create base")?;

        Ok([head, base])
    }

    fn get_source_and_command<'s>(&'s self) -> [Option<(Source, Command)>; 2] {
        let split = |s: &'s str| -> (Source, Command) {
            if let Some(s) = s.strip_prefix('@') {
                if let Some((source, cmd)) = s.split_once(' ') {
                    (Some(source), Some(cmd))
                } else {
                    (Some(s), None)
                }
            } else {
                (None, Some(s))
            }
        };

        let base = self.base.as_deref().map(split);

        let head = self.head.as_deref().map(split);
        [base, head]
    }

    pub fn svd2rust_setup(&self, opts: &Opts) -> Result<[(PathBuf, Command); 2], anyhow::Error> {
        // FIXME: refactor this to be less ugly
        let [base_sc, head_sc] = self.get_source_and_command();
        let base = match base_sc.and_then(|(source, _)| source) {
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
            Some("debug") => crate::get_cargo_metadata()
                .target_directory
                .join(format!("debug/svd2rust{}", std::env::consts::EXE_SUFFIX)),
            Some(reference) => github::get_release_binary_artifact(reference, &opts.output_dir)
                .with_context(|| format!("could not get svd2rust for {reference}"))?,
        };

        let head = match head_sc.and_then(|(source, _)| source) {
            None | Some("" | "pr") => {
                let (number, sha) =
                    github::get_current_pr().with_context(|| "couldn't get current pr")?;
                github::get_pr_binary_artifact(number, &sha, &opts.output_dir)
                    .with_context(|| "couldn't get pr artifact")?
            }
            Some("debug") => crate::get_cargo_metadata()
                .target_directory
                .join(format!("debug/svd2rust{}", std::env::consts::EXE_SUFFIX)),
            Some(reference) => github::get_release_binary_artifact(reference, &opts.output_dir)
                .with_context(|| format!("could not get svd2rust for {reference}"))?,
        };

        Ok([
            (base, base_sc.and_then(|(_, cmd)| cmd)),
            (head, head_sc.and_then(|(_, cmd)| cmd)),
        ])
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
