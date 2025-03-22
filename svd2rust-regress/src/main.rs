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
use std::path::PathBuf;
use std::process::{exit, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use svd_test::WorkspaceTomlGuard;
use wildmatch::WildMatch;

#[derive(Debug, serde::Deserialize)]
pub struct CargoMetadata {
    workspace_root: PathBuf,
    target_directory: PathBuf,
}

static RUSTFMT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static FORM: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Returns the cargo metadata
pub fn get_cargo_metadata() -> &'static CargoMetadata {
    static WORKSPACE: std::sync::OnceLock<CargoMetadata> = std::sync::OnceLock::new();
    WORKSPACE.get_or_init(|| {
        std::process::Command::new("cargo")
            .args(["metadata", "--format-version", "1"])
            .output()
            .map(|v| String::from_utf8(v.stdout))
            .unwrap()
            .map_err(anyhow::Error::from)
            .and_then(|s: String| serde_json::from_str::<CargoMetadata>(&s).map_err(Into::into))
            .unwrap()
    })
}

/// Returns the cargo workspace for the manifest
#[must_use]
pub fn get_cargo_workspace() -> &'static std::path::Path {
    &get_cargo_metadata().workspace_root
}

#[derive(clap::Parser, Debug)]
pub struct TestAll {
    /// Run a long test (it's very long)
    #[clap(short = 'l', long)]
    pub long_test: bool,

    /// Filter by chip name, case sensitive, may be combined with other filters
    #[clap(short = 'c', long)]
    pub chip: Vec<String>,

    /// Filter by manufacturer, may be combined with other filters
    #[clap(
        short = 'm',
        long = "manufacturer",
        ignore_case = true,
        value_parser = manufacturers(),
    )]
    pub mfgr: Option<String>,

    /// Filter by architecture, may be combined with other filters
    #[clap(
        short = 'a',
        long = "architecture",
        ignore_case = true,
        value_parser = architectures(),
    )]
    pub arch: Option<String>,

    /// Include tests expected to fail (will cause a non-zero return code)
    #[clap(short = 'b', long)]
    pub bad_tests: bool,

    /// Enable formatting with `rustfmt`
    #[clap(short = 'f', long)]
    pub format: bool,

    #[clap(long)]
    /// Enable splitting `lib.rs` with `form`
    pub form_lib: bool,

    /// Check generated crates with clippy.
    #[clap(long)]
    pub clippy: bool,

    /// Check documentation build with stable.
    #[clap(long)]
    pub docs_stable: bool,

    /// Check documentation build with nightly settings (docs.rs equivalent).
    #[clap(long)]
    pub docs_nightly: bool,

    /// Print all available test using the specified filters
    #[clap(long)]
    pub list: bool,

    /// Path to an `svd2rust` binary, relative or absolute.
    /// Defaults to `target/release/svd2rust[.exe]` of this repository
    /// (which must be already built)
    #[clap(short = 'p', long = "svd2rust-path", default_value = default_svd2rust())]
    pub current_bin_path: PathBuf,
    #[clap(last = true)]
    pub passthrough_opts: Option<Vec<String>>,
    // TODO: Specify smaller subset of tests? Maybe with tags?
    // TODO: Compile svd2rust?
}

#[derive(clap::Parser, Debug)]
// TODO: Replace with https://github.com/clap-rs/clap/issues/2621 when available
#[group(id = "svd_source", required = true)]
pub struct Test {
    /// Enable formatting with `rustfmt`
    #[arg(short = 'f', long)]
    pub format: bool,

    #[arg(long)]
    /// Enable splitting `lib.rs` with `form`
    pub form_lib: bool,

    #[arg(
        short = 'm',
        long = "manufacturer",
        ignore_case = true,
        value_parser = manufacturers(),
    )]
    /// Manufacturer
    pub mfgr: Option<String>,
    #[arg(
        short = 'a',
        long = "architecture",
        ignore_case = true,
        value_parser = architectures(),
    )]
    /// Architecture
    pub arch: Option<String>,
    #[arg(long, group = "svd_source", conflicts_with_all = ["svd_file"], requires = "arch")]
    /// URL to SVD file to test
    pub url: Option<String>,
    #[arg(long = "svd", group = "svd_source")]
    /// Path to SVD file to test
    pub svd_file: Option<PathBuf>,
    #[arg(short = 'c', long, group = "svd_source")]
    /// Chip to use, use `--url` or `--svd-file` for another way to specify svd
    pub chip: Option<String>,

    /// Check generated crate with clippy.
    #[arg(long)]
    pub clippy: bool,

    /// Check documentation build with stable.
    #[clap(long)]
    pub docs_stable: bool,

    /// Check documentation build with nightly settings (docs.rs equivalent).
    #[clap(long)]
    pub docs_nightly: bool,

    /// Path to an `svd2rust` binary, relative or absolute.
    /// Defaults to `target/release/svd2rust[.exe]` of this repository
    /// (which must be already built)
    #[clap(short = 'p', long = "svd2rust-path", default_value = default_svd2rust())]
    pub current_bin_path: PathBuf,
    #[clap(last = true)]
    pub passthrough_opts: Option<Vec<String>>,
}

impl Test {
    fn run(&self, opts: &Opts) -> Result<(), anyhow::Error> {
        match self {
            Self { url: Some(_), .. } => {}
            Self {
                svd_file: Some(_), ..
            } => {}
            Self { chip: Some(_), .. } => {}
            _ => unreachable!("clap should not allow this"),
        }
        let _toml_guard = WorkspaceTomlGuard::new()?;
        let test = if let (Some(url), Some(arch)) = (&self.url, &self.arch) {
            tests::TestCase {
                arch: svd2rust::Target::parse(arch)?,
                mfgr: tests::Manufacturer::Unknown,
                chip: self
                    .chip
                    .as_deref()
                    .or_else(|| url.rsplit('/').next().and_then(|s| s.strip_suffix(".svd")))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "could not figure out chip name, specify with `--chip <name>`",
                        )
                    })?
                    .to_owned(),
                svd_url: Some(url.clone()),
                should_pass: true,
                skip_check: false,
                suffix: Default::default(),
                opts: Default::default(),
                run_when: tests::RunWhen::default(),
            }
        } else {
            tests::tests(Some(&opts.test_cases))?
                .iter()
                .find(|t| self.chip.iter().any(|c| WildMatch::new(c).matches(&t.chip)))
                .ok_or_else(|| anyhow::anyhow!("no test found for chip"))?
                .to_owned()
        };
        test.test(
            opts,
            &self.current_bin_path,
            self.clippy,
            self.docs_stable,
            self.docs_nightly,
            &self.passthrough_opts,
        )?;
        Ok(())
    }
}

impl TestAll {
    fn run(&self, opt: &Opts) -> Result<(), anyhow::Error> {
        let tests = tests::tests(Some(&opt.test_cases))?
            .iter()
            // Short test?
            .filter(|t| t.should_run(!self.long_test))
            // selected architecture?
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
            // Specify chip - note: may match multiple
            .filter(|t| {
                if self.chip.is_empty() {
                    // Don't run failable tests unless wanted
                    self.bad_tests || t.should_pass
                } else {
                    self.chip.iter().any(|c| WildMatch::new(c).matches(&t.chip))
                }
            })
            .collect::<Vec<_>>();
        if self.list {
            // FIXME: Prettier output
            println!("{:?}", tests.iter().map(|t| t.name()).collect::<Vec<_>>());
            exit(0);
        }
        if tests.is_empty() {
            tracing::error!(
                "No tests run, you might want to use `--bad-tests` and/or `--long-test`"
            );
        }

        let toml_guard = WorkspaceTomlGuard::new()?;

        let any_fails = AtomicBool::new(false);
        tests.par_iter().for_each(|t| {
            let start = Instant::now();

            match t.test(
                opt,
                &self.current_bin_path,
                self.clippy,
                self.docs_stable,
                self.docs_nightly,
                &self.passthrough_opts,
            ) {
                Ok(s) => {
                    if let Some(stderrs) = s {
                        let mut buf = String::new();
                        for stderr in stderrs {
                            read_file(&stderr, &mut buf);
                        }
                        tracing::info!(
                            "Passed: {} - {} seconds\n{}",
                            t.name(),
                            start.elapsed().as_secs(),
                            buf
                        );
                    } else {
                        tracing::info!(
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
                            _ => String::new(),
                        }
                    } else {
                        String::new()
                    };
                    tracing::error!(
                        "Failed: {} - {} seconds. {:?}{}",
                        t.name(),
                        start.elapsed().as_secs(),
                        anyhow::Error::new(e),
                        additional_info,
                    );
                }
            }
        });
        drop(toml_guard);

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
    Tests(TestAll),
    Test(Test),
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

    /// Path to a `form` binary, relative or absolute.
    /// Defaults to `form`
    #[clap(global = true, long)]
    pub form_bin_path: Option<PathBuf>,

    /// Specify what rustup toolchain to use when compiling chip(s)
    #[clap(global = true, long = "toolchain")] // , env = "RUSTUP_TOOLCHAIN"
    pub rustup_toolchain: Option<String>,

    /// Test cases to run
    #[clap(global = true, long, default_value = default_test_cases())]
    pub test_cases: std::path::PathBuf,

    #[clap(global = true, long, short, default_value = "output")]
    pub output_dir: std::path::PathBuf,

    #[clap(subcommand)]
    subcommand: Subcommand,
}

impl Opts {
    const fn use_rustfmt(&self) -> bool {
        match self.subcommand {
            Subcommand::Tests(TestAll { format, .. })
            | Subcommand::Test(Test { format, .. })
            | Subcommand::Diff(Diffing { format, .. })
            | Subcommand::Ci(Ci { format, .. }) => format,
        }
    }

    const fn use_form(&self) -> bool {
        match self.subcommand {
            Subcommand::Tests(TestAll { form_lib, .. })
            | Subcommand::Test(Test { form_lib, .. })
            | Subcommand::Diff(Diffing {
                form_split: form_lib,
                ..
            })
            | Subcommand::Ci(Ci { form_lib, .. }) => form_lib,
        }
    }
}

/// Hack to use svd2rust-regress/tests.yml as default value when running as `cargo run`
fn default_test_cases() -> std::ffi::OsString {
    std::env::var_os("CARGO_MANIFEST_DIR").map_or_else(
        || std::ffi::OsString::from("tests.yml".to_owned()),
        |path| {
            let path = std::path::PathBuf::from(path);
            let path = path.join("tests.yml");
            path.to_owned().into_os_string()
        },
    )
}

fn default_svd2rust() -> std::ffi::OsString {
    get_cargo_workspace()
        .join(format!(
            "target/release/svd2rust{}",
            std::env::consts::EXE_SUFFIX,
        ))
        .into_os_string()
}

fn architectures() -> Vec<clap::builder::PossibleValue> {
    svd2rust::Target::all()
        .iter()
        .map(|arch| clap::builder::PossibleValue::new(arch.to_string()))
        .collect()
}

fn manufacturers() -> Vec<clap::builder::PossibleValue> {
    tests::Manufacturer::all()
        .iter()
        .map(|mfgr| clap::builder::PossibleValue::new(mfgr.to_string()))
        .collect()
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
            tracing::info!("{} is not unique!", name);
        }
    }

    assert!(!fail, "Tests failed validation");
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
    tracing_subscriber::fmt()
        .pretty()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    // Validate all test pre-conditions
    validate_tests(tests::tests(Some(&opt.test_cases))?);

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

    match (&opt.rustfmt_bin_path, opt.use_rustfmt()) {
        (_, false) => {}
        (Some(path), true) => {
            RUSTFMT.get_or_init(|| path.clone());
        }
        (&None, true) => {
            // FIXME: Use Option::filter instead when stable, rust-lang/rust#45860
            assert!(
                default_rustfmt.iter().any(|p| p.is_file()),
                "No rustfmt found"
            );
            if let Some(default_rustfmt) = default_rustfmt {
                RUSTFMT.get_or_init(|| default_rustfmt);
            }
        }
    };
    match (&opt.form_bin_path, opt.use_form()) {
        (_, false) => {}
        (Some(path), true) => {
            FORM.get_or_init(|| path.clone());
        }
        (&None, true) => {
            if let Ok(form) = which::which("form") {
                FORM.get_or_init(|| form);
            }
        }
    }

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

            test_opts.run(&opt).with_context(|| "failed to run tests")
        }
        Subcommand::Diff(diff) => diff.run(&opt).with_context(|| "failed to run diff"),
        Subcommand::Ci(ci) => ci.run(&opt).with_context(|| "failed to run ci"),
        Subcommand::Test(test) => {
            anyhow::ensure!(
                test.current_bin_path.exists(),
                "svd2rust binary does not exist"
            );
            test.run(&opt).with_context(|| "failed to run test")
        }
    }
}

macro_rules! gha_output {
    ($fmt:literal$(, $args:expr)* $(,)?) => {
        #[cfg(not(test))]
        println!($fmt $(, $args)*);
        #[cfg(test)]
        eprintln!($fmt $(,$args)*);
    };
}

pub fn gha_print(content: &str) {
    gha_output!("{}", content);
}

pub fn gha_error(content: &str) {
    gha_output!("::error {}", content);
}

#[track_caller]
pub fn gha_output(tag: &str, content: &str) -> anyhow::Result<()> {
    if content.contains('\n') {
        // https://github.com/actions/toolkit/issues/403
        anyhow::bail!("output `{tag}` contains newlines, consider serializing with json and deserializing in gha with fromJSON()");
    }
    write_to_gha_env_file("GITHUB_OUTPUT", &format!("{tag}={content}"))?;
    Ok(())
}

// https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions#environment-files
pub fn write_to_gha_env_file(env_name: &str, contents: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let path = std::env::var(env_name)?;
    let path = std::path::Path::new(&path);
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    writeln!(file, "{}", contents)?;
    Ok(())
}
