use std::path::PathBuf;

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
    #[clap(global = true, long, alias = "base")]
    pub baseline: Option<String>,

    #[clap(global = true, long, alias = "head")]
    pub current: Option<String>,

    /// Enable formatting with `rustfmt`
    #[clap(global = true, short = 'f', long)]
    pub format: bool,

    /// Enable splitting `lib.rs` with `form`
    #[clap(global = true, long)]
    pub form_split: bool,

    #[clap(subcommand)]
    pub sub: Option<DiffingMode>,

    #[clap(long, short = 'c')]
    pub chip: Vec<String>,

    /// Filter by manufacturer, case sensitive, may be combined with other filters
    #[clap(
            short = 'm',
            long = "manufacturer",
            ignore_case = true,
            value_parser = crate::manufacturers(),
        )]
    pub mfgr: Option<String>,

    /// Filter by architecture, case sensitive, may be combined with other filters
    #[clap(
            short = 'a',
            long = "architecture",
            ignore_case = true,
            value_parser = crate::architectures(),
        )]
    pub arch: Option<String>,

    #[clap(global = true, long)]
    pub diff_folder: Option<PathBuf>,

    #[clap(last = true)]
    pub args: Option<String>,
}

#[derive(clap::Parser, Debug, Clone, Copy)]
pub enum DiffingMode {
    Semver,
    Diff,
}

type Source<'s> = Option<&'s str>;
type Command<'s> = Option<&'s str>;

impl Diffing {
    pub fn run(&self, opts: &Opts) -> Result<(), anyhow::Error> {
        let [baseline, current] = self
            .make_case(opts)
            .with_context(|| "couldn't setup test case")?;
        match self.sub.unwrap_or(DiffingMode::Diff) {
            DiffingMode::Diff => std::process::Command::new("git")
                .args(["--no-pager", "diff", "--no-index", "--minimal"])
                .args([&*baseline.0, &*current.0])
                .status()
                .with_context(|| "couldn't run git diff")
                .map(|_| ()),
            DiffingMode::Semver => std::process::Command::new("cargo")
                .args(["semver-checks", "check-release"])
                .arg("--baseline-root")
                .arg(baseline.0)
                .arg("--manifest-path")
                .arg(current.0.join("Cargo.toml"))
                .status()
                .with_context(|| "couldn't run git diff")
                .map(|_| ()),
        }
    }

    pub fn make_case(&self, opts: &Opts) -> Result<[(PathBuf, Vec<PathBuf>); 2], anyhow::Error> {
        let [(baseline_bin, baseline_cmd), (current_bin, current_cmd)] = self
            .svd2rust_setup(opts)
            .with_context(|| "couldn't setup svd2rust")?;
        let tests = crate::tests::tests(Some(opts.test_cases.as_ref()))
            .with_context(|| "no tests found")?;

        let tests = tests
            .iter()
            .filter(|t| {
                if let Some(ref arch) = self.arch {
                    arch.to_ascii_lowercase()
                        .eq_ignore_ascii_case(&t.arch.to_string())
                } else {
                    true
                }
            })
            // selected manufacturer?
            .filter(|t| {
                if let Some(ref mfgr) = self.mfgr {
                    mfgr.to_ascii_lowercase()
                        .eq_ignore_ascii_case(&t.mfgr.to_string().to_ascii_lowercase())
                } else {
                    true
                }
            })
            .filter(|t| {
                if !self.chip.is_empty() {
                    self.chip.iter().any(|c| {
                        wildmatch::WildMatch::new(&c.to_ascii_lowercase())
                            .matches(&t.chip.to_ascii_lowercase())
                    })
                } else {
                    false
                }
            })
            .collect::<Vec<_>>();
        if tests.len() != 1 {
            let error = anyhow::anyhow!("diff requires exactly one test case");
            if tests.is_empty() {
                return Err(error.context("matched no tests"));
            } else if tests.len() > 10 {
                return Err(error.context(format!("matched multiple ({}) tests", tests.len())));
            }
            return Err(error.context(format!(
                "matched multiple ({}) tests\n{:?}",
                tests.len(),
                tests.iter().map(|t| t.name()).collect::<Vec<_>>()
            )));
        }

        let baseline = tests[0]
            .setup_case(
                &opts.output_dir.join("baseline"),
                &baseline_bin,
                baseline_cmd,
            )
            .with_context(|| "couldn't create head")?;
        let current = tests[0]
            .setup_case(&opts.output_dir.join("current"), &current_bin, current_cmd)
            .with_context(|| "couldn't create base")?;

        Ok([baseline, current])
    }

    fn get_source_and_command<'s>(&'s self) -> [Option<(Source, Command)>; 2] {
        let split = |s: &'s str| -> (Source, Command) {
            if let Some(s) = s.strip_prefix('@') {
                if let Some((source, cmd)) = s.split_once(' ') {
                    (Some(source), Some(cmd.trim()))
                } else {
                    (Some(s), None)
                }
            } else {
                (None, Some(s.trim()))
            }
        };

        let baseline = self.baseline.as_deref().map(split);

        let current = self.current.as_deref().map(split);
        [baseline, current]
    }

    pub fn svd2rust_setup(&self, opts: &Opts) -> Result<[(PathBuf, Command); 2], anyhow::Error> {
        // FIXME: refactor this to be less ugly
        let [baseline_sc, current_sc] = self.get_source_and_command();
        let baseline = match baseline_sc.and_then(|(source, _)| source) {
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
            Some("release") => crate::get_cargo_metadata()
                .target_directory
                .join(format!("release/svd2rust{}", std::env::consts::EXE_SUFFIX)),
            Some(reference) => github::get_release_binary_artifact(reference, &opts.output_dir)
                .with_context(|| format!("could not get svd2rust for {reference}"))?,
        };

        let current = match current_sc.and_then(|(source, _)| source) {
            None | Some("" | "pr") => {
                let (number, sha) =
                    github::get_current_pr().with_context(|| "couldn't get current pr")?;
                github::get_pr_binary_artifact(number, &sha, &opts.output_dir)
                    .with_context(|| "couldn't get pr artifact")?
            }
            Some("debug") => crate::get_cargo_metadata()
                .target_directory
                .join(format!("debug/svd2rust{}", std::env::consts::EXE_SUFFIX)),
            Some("release") => crate::get_cargo_metadata()
                .target_directory
                .join(format!("release/svd2rust{}", std::env::consts::EXE_SUFFIX)),
            Some(reference) => github::get_release_binary_artifact(reference, &opts.output_dir)
                .with_context(|| format!("could not get svd2rust for {reference}"))?,
        };

        Ok([
            (baseline, baseline_sc.and_then(|(_, cmd)| cmd)),
            (current, current_sc.and_then(|(_, cmd)| cmd)),
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
