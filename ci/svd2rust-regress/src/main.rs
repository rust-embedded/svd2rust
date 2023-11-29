pub mod ci;
pub mod command;
pub mod diff;
pub mod github;
mod svd_test;
mod tests;

use anyhow::Context;
use ci::Ci;
use diff::Diffing;

use clap::Parser;
use rayon::prelude::*;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use wildmatch::WildMatch;

/// Returns the cargo workspace for the manifest
pub fn get_cargo_workspace() -> &'static std::path::Path {
    static WORKSPACE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    #[derive(Debug, serde::Deserialize)]
    pub struct CargoMetadata {
        pub workspace_root: PathBuf,
    }
    WORKSPACE.get_or_init(|| {
        std::process::Command::new("cargo")
            .args(["metadata", "--format-version", "1"])
            .output()
            .map(|v| String::from_utf8(v.stdout))
            .unwrap()
            .map_err(anyhow::Error::from)
            .and_then(|s: String| serde_json::from_str::<CargoMetadata>(&s).map_err(Into::into))
            .unwrap()
            .workspace_root
    })
}

#[derive(clap::Parser, Debug)]
pub struct TestOpts {
    /// Run a long test (it's very long)
    #[clap(short = 'l', long)]
    pub long_test: bool,

    // TODO: Consider using the same strategy cargo uses for passing args to rustc via `--`
    /// Run svd2rust with `--atomics`
    #[clap(long)]
    pub atomics: bool,

    /// Filter by chip name, case sensitive, may be combined with other filters
    #[clap(short = 'c', long)]
    pub chip: Vec<String>,

    /// Filter by manufacturer, case sensitive, may be combined with other filters
    #[clap(
    short = 'm',
    long = "manufacturer",
    value_parser = validate_manufacturer,
)]
    pub mfgr: Option<String>,

    /// Filter by architecture, case sensitive, may be combined with other filters
    /// Options are: "CortexM", "RiscV", "Msp430", "Mips" and "XtensaLX"
    #[clap(
    short = 'a',
    long = "architecture",
    value_parser = validate_architecture,
)]
    pub arch: Option<String>,

    /// Include tests expected to fail (will cause a non-zero return code)
    #[clap(short = 'b', long)]
    pub bad_tests: bool,

    /// Enable formatting with `rustfmt`
    #[clap(short = 'f', long)]
    pub format: bool,

    /// Print all available test using the specified filters
    #[clap(long)]
    pub list: bool,

    /// Path to an `svd2rust` binary, relative or absolute.
    /// Defaults to `target/release/svd2rust[.exe]` of this repository
    /// (which must be already built)
    #[clap(short = 'p', long = "svd2rust-path", default_value = default_svd2rust())]
    pub current_bin_path: PathBuf,
    // TODO: Specify smaller subset of tests? Maybe with tags?
    // TODO: Compile svd2rust?
}

impl TestOpts {
    fn run(&self, opt: &Opts, rustfmt_bin_path: Option<&Path>) -> Result<(), anyhow::Error> {
        let tests = tests::tests(None)?
            .iter()
            // Short test?
            .filter(|t| t.should_run(!self.long_test))
            // selected architecture?
            .filter(|t| {
                if let Some(ref arch) = self.arch {
                    WildMatch::new(arch).matches(&format!("{:?}", t.arch))
                } else {
                    true
                }
            })
            // selected manufacturer?
            .filter(|t| {
                if let Some(ref mfgr) = self.mfgr {
                    WildMatch::new(mfgr).matches(&format!("{:?}", t.mfgr))
                } else {
                    true
                }
            })
            // Specify chip - note: may match multiple
            .filter(|t| {
                if !self.chip.is_empty() {
                    self.chip.iter().any(|c| WildMatch::new(c).matches(&t.chip))
                } else {
                    // Don't run failable tests unless wanted
                    self.bad_tests || t.should_pass
                }
            })
            .collect::<Vec<_>>();
        if self.list {
            // FIXME: Prettier output
            eprintln!("{:?}", tests.iter().map(|t| t.name()).collect::<Vec<_>>());
            exit(0);
        }
        if tests.is_empty() {
            eprintln!("No tests run, you might want to use `--bad-tests` and/or `--long-test`");
        }
        let any_fails = AtomicBool::new(false);
        tests.par_iter().for_each(|t| {
            let start = Instant::now();

            match t.test(opt, self, rustfmt_bin_path) {
                Ok(s) => {
                    if let Some(stderrs) = s {
                        let mut buf = String::new();
                        for stderr in stderrs {
                            read_file(&stderr, &mut buf);
                        }
                        eprintln!(
                            "Passed: {} - {} seconds\n{}",
                            t.name(),
                            start.elapsed().as_secs(),
                            buf
                        );
                    } else {
                        eprintln!(
                            "Passed: {} - {} seconds",
                            t.name(),
                            start.elapsed().as_secs()
                        );
                    }
                }
                Err(e) => {
                    any_fails.store(true, Ordering::Release);
                    let additional_info = if opt.verbose > 0 {
                        match &e {
                            svd_test::TestError::Process(svd_test::ProcessFailed {
                                stderr: Some(ref stderr),
                                previous_processes_stderr,
                                ..
                            }) => {
                                let mut buf = String::new();
                                if opt.verbose > 1 {
                                    for stderr in previous_processes_stderr {
                                        read_file(stderr, &mut buf);
                                    }
                                }
                                read_file(stderr, &mut buf);
                                buf
                            }
                            _ => "".into(),
                        }
                    } else {
                        "".into()
                    };
                    eprintln!(
                        "Failed: {} - {} seconds. {:?}{}",
                        t.name(),
                        start.elapsed().as_secs(),
                        anyhow::Error::new(e),
                        additional_info,
                    );
                }
            }
        });
        if any_fails.load(Ordering::Acquire) {
            exit(1);
        } else {
            exit(0);
        }
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum Subcommand {
    Diff(Diffing),
    Tests(TestOpts),
    Ci(Ci),
}

#[derive(Parser, Debug)]
#[command(name = "svd2rust-regress")]
pub struct Opts {
    /// Use verbose output
    #[clap(global = true, long, short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Path to an `rustfmt` binary, relative or absolute.
    /// Defaults to `$(rustup which rustfmt)`
    #[clap(global = true, long)]
    pub rustfmt_bin_path: Option<PathBuf>,

    /// Specify what rustup toolchain to use when compiling chip(s)
    #[clap(global = true, long = "toolchain")] // , env = "RUSTUP_TOOLCHAIN"
    pub rustup_toolchain: Option<String>,

    /// Test cases to run, defaults to `tests.json`
    #[clap(global = true, long, default_value = default_test_cases())]
    pub test_cases: std::path::PathBuf,

    #[clap(global = true, long, short, default_value = "output")]
    pub output_dir: std::path::PathBuf,

    #[clap(subcommand)]
    subcommand: Subcommand,
}

impl Opts {
    fn use_rustfmt(&self) -> bool {
        match self.subcommand {
            Subcommand::Tests(TestOpts { format, .. }) => format,
            Subcommand::Diff(Diffing { format, .. }) => format,
            Subcommand::Ci(Ci { format, .. }) => format,
        }
    }
}

/// Hack to use ci/tests.json as default value when running as `cargo run`
fn default_test_cases() -> std::ffi::OsString {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map(|mut e| {
            e.extend([std::ffi::OsStr::new("/tests.json")]);
            std::path::PathBuf::from(e)
                .strip_prefix(std::env::current_dir().unwrap())
                .unwrap()
                .to_owned()
                .into_os_string()
        })
        .unwrap_or_else(|| std::ffi::OsString::from("tests.json".to_owned()))
}

fn default_svd2rust() -> std::ffi::OsString {
    get_cargo_workspace()
        .join(format!(
            "target/release/svd2rust{}",
            std::env::consts::EXE_SUFFIX,
        ))
        .into_os_string()
}

fn validate_architecture(s: &str) -> Result<(), anyhow::Error> {
    if tests::tests(None)?
        .iter()
        .any(|t| format!("{:?}", t.arch) == s)
    {
        Ok(())
    } else {
        anyhow::bail!("Architecture `{s}` is not a valid value")
    }
}

fn validate_manufacturer(s: &str) -> Result<(), anyhow::Error> {
    if tests::tests(None)?
        .iter()
        .any(|t| format!("{:?}", t.mfgr) == s)
    {
        Ok(())
    } else {
        anyhow::bail!("Manufacturer `{s}` is not a valid value")
    }
}

/// Validate any assumptions made by this program
fn validate_tests(tests: &[tests::TestCase]) {
    use std::collections::HashSet;

    let mut fail = false;

    // CONDITION 1: All mfgr+chip names must be unique
    let mut uniq = HashSet::new();
    for t in tests {
        let name = t.name();
        if !uniq.insert(name.clone()) {
            fail = true;
            eprintln!("{} is not unique!", name);
        }
    }

    if fail {
        panic!("Tests failed validation");
    }
}

fn read_file(path: &PathBuf, buf: &mut String) {
    if buf.is_empty() {
        buf.push_str(&format!("{}\n", path.display()));
    } else {
        buf.push_str(&format!("\n{}\n", path.display()));
    }
    File::open(path)
        .expect("Couldn't open file")
        .read_to_string(buf)
        .expect("Couldn't read file to string");
}

fn main() -> Result<(), anyhow::Error> {
    let opt = Opts::parse();

    // Validate all test pre-conditions
    validate_tests(tests::tests(Some(&opt))?);

    let default_rustfmt: Option<PathBuf> = if let Some((v, true)) = Command::new("rustup")
        .args(["which", "rustfmt"])
        .output()
        .ok()
        .map(|o| (o.stdout, o.status.success()))
    {
        Some(String::from_utf8_lossy(&v).into_owned().trim().into())
    } else {
        None
    };

    let rustfmt_bin_path = match (&opt.rustfmt_bin_path, opt.use_rustfmt()) {
        (_, false) => None,
        (Some(path), true) => Some(path.as_path()),
        (&None, true) => {
            // FIXME: Use Option::filter instead when stable, rust-lang/rust#45860
            if !default_rustfmt.iter().any(|p| p.is_file()) {
                panic!("No rustfmt found");
            }
            default_rustfmt.as_deref()
        }
    };

    // Set RUSTUP_TOOLCHAIN if needed
    if let Some(toolchain) = &opt.rustup_toolchain {
        std::env::set_var("RUSTUP_TOOLCHAIN", toolchain);
    }

    match &opt.subcommand {
        Subcommand::Tests(test_opts) => {
            anyhow::ensure!(
                test_opts.current_bin_path.exists(),
                "svd2rust binary does not exist"
            );

            test_opts
                .run(&opt, rustfmt_bin_path)
                .with_context(|| "failed to run tests")
        }
        Subcommand::Diff(diff) => diff
            .run(&opt, rustfmt_bin_path)
            .with_context(|| "failed to run diff"),
        Subcommand::Ci(ci) => ci
            .run(&opt, rustfmt_bin_path)
            .with_context(|| "failed to run ci"),
    }
}
