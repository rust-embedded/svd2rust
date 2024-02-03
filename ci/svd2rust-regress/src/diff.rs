use std::path::PathBuf;

use anyhow::Context;

use crate::github;
use crate::Opts;

#[derive(clap::Parser, Debug)]
#[clap(name = "diff")]
pub struct Diffing {
    /// The base version of svd2rust to use and the command input, defaults to latest master build: `@master`
    ///
    /// Change the base version by starting with `@` followed by the source.
    ///
    /// supports `@pr` for current pr, `@master` for latest master build, or a version tag like `@v0.30.0`
    #[clap(global = true, long = "baseline", alias = "base")]
    pub baseline: Option<String>,

    /// The head version of svd2rust to use and the command input, defaults to current pr: `@pr`
    #[clap(global = true, long = "current", alias = "head")]
    pub current: Option<String>,

    /// Enable formatting with `rustfmt`
    #[clap(global = true, short = 'f', long)]
    pub format: bool,

    /// Enable splitting `lib.rs` with `form`
    #[clap(global = true, long)]
    pub form_split: bool,

    #[clap(global = true, long, short = 'c')]
    pub chip: Vec<String>,

    /// Filter by manufacturer, case sensitive, may be combined with other filters
    #[clap(
            global = true,
            short = 'm',
            long = "manufacturer",
            ignore_case = true,
            value_parser = crate::manufacturers(),
        )]
    pub mfgr: Option<String>,

    /// Filter by architecture, case sensitive, may be combined with other filters
    #[clap(
            global = true,
            short = 'a',
            long = "architecture",
            ignore_case = true,
            value_parser = crate::architectures(),
        )]
    pub arch: Option<String>,

    #[clap(global = true, long)]
    pub diff_folder: Option<PathBuf>,

    /// The pr number to use for `@pr`. If not set will try to get the current pr with the command `gh pr`
    #[clap(env = "GITHUB_PR", global = true, long)]
    pub pr: Option<usize>,

    #[clap(env = "GIT_PAGER", global = true, long)]
    pub pager: Option<String>,

    /// if set, will use pager directly instead of `git -c core.pager`
    #[clap(global = true, long, short = 'P')]
    pub use_pager_directly: bool,

    /// URL for SVD to download
    #[clap(global = true, long)]
    pub url: Option<String>,

    #[clap(subcommand)]
    pub sub: Option<DiffingMode>,

    #[clap(last = true)]
    pub last_args: Option<String>,
}

#[derive(clap::Parser, Debug, Clone)]
pub enum DiffingMode {
    Semver {
        #[clap(last = true)]
        last_args: Option<String>,
    },
    Diff {
        #[clap(last = true)]
        last_args: Option<String>,
    },
    Pr {
        #[clap(last = true)]
        last_args: Option<String>,
    },
}

impl DiffingMode {
    /// Returns `true` if the diffing mode is [`Pr`].
    ///
    /// [`Pr`]: DiffingMode::Pr
    #[must_use]
    pub fn is_pr(&self) -> bool {
        matches!(self, Self::Pr { .. })
    }
}

type Source<'s> = Option<&'s str>;
type Command<'s> = Option<&'s str>;

impl Diffing {
    pub fn run(&self, opts: &Opts) -> Result<(), anyhow::Error> {
        let [baseline, current] = self
            .make_case(opts)
            .with_context(|| "couldn't setup test case")?;
        match self.sub.as_ref() {
            None | Some(DiffingMode::Diff { .. } | DiffingMode::Pr { .. }) => {
                let mut command;
                if let Some(pager) = &self.pager {
                    if self.use_pager_directly {
                        let mut pager = pager.split_whitespace();
                        command = std::process::Command::new(pager.next().unwrap());
                        command.args(pager);
                    } else {
                        command = std::process::Command::new("git");
                        command.env("GIT_PAGER", pager);
                    }
                } else {
                    command = std::process::Command::new("git");
                    command.arg("--no-pager");
                }
                if !self.use_pager_directly {
                    command.args(["diff", "--no-index", "--minimal"]);
                }
                command
                    .args([&*baseline.0, &*current.0])
                    .status()
                    .with_context(|| "couldn't run diff")
                    .map(|_| ())
            }
            Some(DiffingMode::Semver { .. }) => std::process::Command::new("cargo")
                .args(["semver-checks", "check-release"])
                .arg("--baseline-root")
                .arg(baseline.0)
                .arg("--manifest-path")
                .arg(current.0.join("Cargo.toml"))
                .status()
                .with_context(|| "couldn't run semver-checks")
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
                if self.chip.is_empty() {
                    true
                } else {
                    self.chip.iter().any(|c| {
                        wildmatch::WildMatch::new(&c.to_ascii_lowercase())
                            .matches(&t.chip.to_ascii_lowercase())
                    })
                }
            })
            .collect::<Vec<_>>();

        let test = match (tests.len(), self.sub.as_ref(), self.url.as_ref()) {
            (1, _, None) => tests[0].clone(),
            (_, Some(DiffingMode::Pr { .. }), None) => tests
                .iter()
                .find(|t| t.chip == "STM32F103")
                .map(|t| (*t).clone())
                .unwrap_or_else(|| tests[0].clone()),
            (_, _, Some(url)) => crate::tests::TestCase {
                arch: self
                    .arch
                    .clone()
                    .map(|s| svd2rust::Target::parse(&s))
                    .transpose()?
                    .unwrap_or_default(),
                mfgr: crate::tests::Manufacturer::Unknown,
                chip: url
                    .rsplit('/')
                    .next()
                    .and_then(|file| file.split('.').next())
                    .ok_or_else(|| anyhow::anyhow!("couldn't get chip name from url"))?
                    .to_owned(),
                svd_url: Some(url.to_owned()),
                should_pass: true,
                run_when: crate::tests::RunWhen::Always,
            },
            _ => {
                let error = anyhow::anyhow!("diff requires exactly one test case");
                let len = tests.len();
                return Err(match len {
                    0 => error.context("matched no tests"),
                    10.. => error.context(format!("matched multiple ({len}) tests")),
                    _ => error.context(format!(
                        "matched multiple ({len}) tests\n{:?}",
                        tests.iter().map(|t| t.name()).collect::<Vec<_>>()
                    )),
                });
            }
        };

        let last_args = self.last_args.as_deref().or(match &self.sub {
            Some(
                DiffingMode::Diff { last_args }
                | DiffingMode::Pr { last_args }
                | DiffingMode::Semver { last_args },
            ) => last_args.as_deref(),
            None => None,
        });
        let join = |opt1: Option<&str>, opt2: Option<&str>| -> Option<String> {
            match (opt1, opt2) {
                (Some(str1), Some(str2)) => Some(format!("{} {}", str1, str2)),
                (Some(str), None) | (None, Some(str)) => Some(str.to_owned()),
                (None, None) => None,
            }
        };
        let baseline = test
            .setup_case(
                &opts.output_dir.join("baseline"),
                &baseline_bin,
                join(baseline_cmd, last_args).as_deref(),
            )
            .with_context(|| "couldn't create head")?;
        let current = test
            .setup_case(
                &opts.output_dir.join("current"),
                &current_bin,
                join(current_cmd, last_args).as_deref(),
            )
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
            reference @ (None | Some("" | "master")) => {
                github::get_release_binary_artifact(reference.unwrap_or("master"), &opts.output_dir)
                    .with_context(|| "couldn't get svd2rust latest unreleased artifact")?
            }
            Some("pr") if self.pr.is_none() => {
                let (number, sha) =
                    github::get_current_pr().with_context(|| "couldn't get current pr")?;
                github::get_pr_binary_artifact(number, &sha, &opts.output_dir)
                    .with_context(|| "couldn't get pr artifact")?
            }
            Some("pr") => {
                let (number, sha) =
                    github::get_pr(self.pr.unwrap()).with_context(|| "couldn't get current pr")?;
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
            None | Some("" | "pr") if self.pr.is_none() => {
                let (number, sha) =
                    github::get_current_pr().with_context(|| "couldn't get current pr")?;
                github::get_pr_binary_artifact(number, &sha, &opts.output_dir)
                    .with_context(|| "couldn't get pr artifact")?
            }
            None | Some("" | "pr") => {
                let (number, sha) =
                    github::get_pr(self.pr.unwrap()).with_context(|| "couldn't get current pr")?;
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
            (
                baseline.canonicalize()?,
                baseline_sc.and_then(|(_, cmd)| cmd),
            ),
            (current.canonicalize()?, current_sc.and_then(|(_, cmd)| cmd)),
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
