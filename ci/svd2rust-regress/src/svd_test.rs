use anyhow::{anyhow, Context, Result};
use svd2rust::{util::Case, Target};

use crate::{command::CommandExt, tests::TestCase, Opts};
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    path::Path,
};

const CRATES_ALL: &[&str] = &[
    "critical-section = {version = \"1.0\", optional = true}",
    "vcell = \"0.1.2\"",
];
const CRATES_MSP430: &[&str] = &["msp430 = \"0.4.0\"", "msp430-rt = \"0.4.0\""];
const CRATES_ATOMICS: &[&str] =
    &["portable-atomic = { version = \"0.3.16\", default-features = false }"];
const CRATES_CORTEX_M: &[&str] = &["cortex-m = \"0.7.6\"", "cortex-m-rt = \"0.7\""];
const CRATES_RISCV: &[&str] = &["riscv = \"0.12.1\"", "riscv-rt = \"0.13.0\""];
const CRATES_XTENSALX: &[&str] = &["xtensa-lx-rt = \"0.9.0\"", "xtensa-lx = \"0.6.0\""];
const CRATES_MIPS: &[&str] = &["mips-mcu = \"0.1.0\""];
const PROFILE_ALL: &[&str] = &["[profile.dev]", "incremental = false"];
const FEATURES_ALL: &[&str] = &["[features]"];
const FEATURES_CORTEX_M: &[&str] = &["rt = [\"cortex-m-rt/device\"]"];
const FEATURES_XTENSALX: &[&str] = &["default = [\"xtensa-lx/esp32\", \"xtensa-lx-rt/esp32\"]"];
const WORKSPACE_EXCLUDE: &[&str] = &["[workspace]"];

fn path_helper_base(base: &Path, input: &[&str]) -> PathBuf {
    input
        .iter()
        .fold(base.to_owned(), |b: PathBuf, p| b.join(p))
}

/// Create and write to file
fn file_helper(payload: &str, path: &Path) -> Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to create {path:?}"))?;

    f.write_all(payload.as_bytes())
        .with_context(|| format!("Failed to write to {path:?}"))?;

    Ok(())
}

#[derive(thiserror::Error)]
#[error("Process failed - {command}")]
pub struct ProcessFailed {
    pub command: String,
    pub stderr: Option<PathBuf>,
    pub stdout: Option<PathBuf>,
    pub previous_processes_stderr: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("test case failed")]
    Process(#[from] ProcessFailed),
    #[error("Failed to run test")]
    Other(#[from] anyhow::Error),
}

impl std::fmt::Debug for ProcessFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Process failed")
    }
}

trait CommandHelper {
    fn capture_outputs(
        &mut self,
        cant_fail: bool,
        name: &str,
        stdout: Option<&PathBuf>,
        stderr: Option<&PathBuf>,
        previous_processes_stderr: &[PathBuf],
    ) -> Result<(), TestError>;
}

impl CommandHelper for Command {
    #[tracing::instrument(skip_all, fields(stdout = tracing::field::Empty, stderr = tracing::field::Empty))]
    fn capture_outputs(
        &mut self,
        cant_fail: bool,
        name: &str,
        stdout: Option<&PathBuf>,
        stderr: Option<&PathBuf>,
        previous_processes_stderr: &[PathBuf],
    ) -> Result<(), TestError> {
        let output = self.get_output(true)?;
        let out_payload = String::from_utf8_lossy(&output.stdout);
        if let Some(out) = stdout {
            file_helper(&out_payload, out)?;
        };

        let err_payload = String::from_utf8_lossy(&output.stderr);
        if let Some(err) = stderr {
            file_helper(&err_payload, err)?;
        };
        if cant_fail && !output.status.success() {
            let span = tracing::Span::current();
            let mut message = format!("Process failed: {}", self.display());
            if !out_payload.trim().is_empty() {
                span.record(
                    "stdout",
                    tracing::field::display(
                        stdout.map(|p| p.display().to_string()).unwrap_or_default(),
                    ),
                );
                write!(message, "\nstdout: \n{}", out_payload).unwrap();
            }
            if !err_payload.trim().is_empty() {
                span.record(
                    "stderr",
                    tracing::field::display(
                        stderr.map(|p| p.display().to_string()).unwrap_or_default(),
                    ),
                );
                write!(message, "\nstderr: \n{}", err_payload).unwrap();
            }
            tracing::error!(message=%message);
            return Err(ProcessFailed {
                command: name.into(),
                stdout: stdout.cloned(),
                stderr: stderr.cloned(),
                previous_processes_stderr: previous_processes_stderr.to_vec(),
            }
            .into());
        }

        Ok(())
    }
}

impl TestCase {
    #[tracing::instrument(skip(self, opts), fields(name = %self.name()))]
    pub fn test(
        &self,
        opts: &Opts,
        bin_path: &Path,
        command: Option<&str>,
    ) -> Result<Option<Vec<PathBuf>>, TestError> {
        let (chip_dir, mut process_stderr_paths) = self
            .setup_case(&opts.output_dir, bin_path, command)
            .with_context(|| anyhow!("when setting up case for {}", self.name()))?;
        // Run `cargo check`, capturing stderr to a log file
        let cargo_check_err_file = path_helper_base(&chip_dir, &["cargo-check.err.log"]);
        Command::new("cargo")
            .arg("check")
            .current_dir(&chip_dir)
            .capture_outputs(
                true,
                "cargo check",
                None,
                Some(&cargo_check_err_file),
                &process_stderr_paths,
            )
            .with_context(|| "failed to check")?;
        process_stderr_paths.push(cargo_check_err_file);
        Ok(if opts.verbose > 1 {
            Some(process_stderr_paths)
        } else {
            None
        })
    }

    #[tracing::instrument(skip(self, output_dir, command), fields(name = %self.name(), chip_dir = tracing::field::Empty))]

    pub fn setup_case(
        &self,
        output_dir: &Path,
        svd2rust_bin_path: &Path,
        command: Option<&str>,
    ) -> Result<(PathBuf, Vec<PathBuf>), TestError> {
        let user = match std::env::var("USER") {
            Ok(val) => val,
            Err(_) => "rusttester".into(),
        };
        let chip_dir = output_dir.join(Case::Snake.sanitize(&self.name()).as_ref());
        tracing::span::Span::current()
            .record("chip_dir", tracing::field::display(chip_dir.display()));
        if let Err(err) = fs::remove_dir_all(&chip_dir) {
            match err.kind() {
                std::io::ErrorKind::NotFound => (),
                _ => Err(err).with_context(|| "While removing chip directory")?,
            }
        }
        let mut process_stderr_paths: Vec<PathBuf> = vec![];
        tracing::info!(
            "Initializing cargo package for `{}` in {}",
            self.name(),
            chip_dir.display()
        );
        // XXX: Workaround for https://github.com/rust-lang/cargo/issues/6009#issuecomment-1925445245
        let manifest_path = crate::get_cargo_workspace().join("Cargo.toml");
        let workspace_toml =
            fs::read(&manifest_path).context("failed to read workspace Cargo.toml")?;
        Command::new("cargo")
            .env("USER", user)
            .arg("init")
            .arg("--name")
            .arg(Case::Snake.sanitize(&self.name()).as_ref())
            .arg("--vcs")
            .arg("none")
            .arg(&chip_dir)
            .capture_outputs(true, "cargo init", None, None, &[])
            .with_context(|| "Failed to cargo init")?;
        std::fs::write(manifest_path, workspace_toml)
            .context("failed to write workspace Cargo.toml")?;

        let svd_toml = path_helper_base(&chip_dir, &["Cargo.toml"]);
        let mut file = OpenOptions::new()
            .append(true)
            .open(svd_toml)
            .with_context(|| "Failed to open Cargo.toml for appending")?;

        let crates = CRATES_ALL
            .iter()
            .chain(match &self.arch {
                Target::CortexM => CRATES_CORTEX_M.iter(),
                Target::RISCV => CRATES_RISCV.iter(),
                Target::Mips => CRATES_MIPS.iter(),
                Target::Msp430 => CRATES_MSP430.iter(),
                Target::XtensaLX => CRATES_XTENSALX.iter(),
                Target::None => unreachable!(),
            })
            .chain(if command.unwrap_or_default().contains("--atomics") {
                CRATES_ATOMICS.iter()
            } else {
                [].iter()
            })
            .chain(PROFILE_ALL.iter())
            .chain(FEATURES_ALL.iter())
            .chain(match &self.arch {
                Target::XtensaLX => FEATURES_XTENSALX.iter(),
                Target::CortexM => FEATURES_CORTEX_M.iter(),
                _ => [].iter(),
            })
            .chain(WORKSPACE_EXCLUDE.iter());
        for c in crates {
            writeln!(file, "{}", c).with_context(|| "Failed to append to file!")?;
        }
        tracing::info!("Downloading SVD");
        // FIXME: Avoid downloading multiple times, especially if we're using the diff command
        let svd_url = &self.svd_url();
        let svd = reqwest::blocking::get(svd_url)
            .with_context(|| format!("Failed to get svd URL: {svd_url}"))?
            .error_for_status()
            .with_context(|| anyhow!("Response is not ok for svd url"))?
            .text()
            .with_context(|| "SVD is bad text")?;

        let chip_svd = format!("{}.svd", &self.chip);
        let svd_file = path_helper_base(&chip_dir, &[&chip_svd]);
        file_helper(&svd, &svd_file)?;
        let lib_rs_file = path_helper_base(&chip_dir, &["src", "lib.rs"]);
        let src_dir = path_helper_base(&chip_dir, &["src"]);
        let svd2rust_err_file = path_helper_base(&chip_dir, &["svd2rust.err.log"]);
        let target = match self.arch {
            Target::CortexM => "cortex-m",
            Target::Msp430 => "msp430",
            Target::Mips => "mips",
            Target::RISCV => "riscv",
            Target::XtensaLX => "xtensa-lx",
            Target::None => unreachable!(),
        };
        tracing::info!("Running svd2rust");
        let mut svd2rust_bin = Command::new(svd2rust_bin_path);
        if let Some(command) = command {
            if !command.is_empty() {
                svd2rust_bin.args(
                    shell_words::split(command).context("unable to split command into args")?,
                );
            }
        }
        svd2rust_bin
            .args(["-i", &chip_svd])
            .args(["--target", target])
            .current_dir(&chip_dir)
            .capture_outputs(
                true,
                "svd2rust",
                Some(&lib_rs_file).filter(|_| {
                    !matches!(
                        self.arch,
                        Target::CortexM | Target::Msp430 | Target::XtensaLX
                    )
                }),
                Some(&svd2rust_err_file),
                &[],
            )?;
        process_stderr_paths.push(svd2rust_err_file);
        match self.arch {
            Target::CortexM | Target::Mips | Target::Msp430 | Target::XtensaLX => {
                // TODO: Give error the path to stderr
                fs::rename(path_helper_base(&chip_dir, &["lib.rs"]), &lib_rs_file)
                    .with_context(|| "While moving lib.rs file")?;
            }
            _ => {}
        }
        let lib_rs =
            fs::read_to_string(&lib_rs_file).with_context(|| "Failed to read lib.rs file")?;
        let file = syn::parse_file(&lib_rs)
            .with_context(|| format!("couldn't parse {}", lib_rs_file.display()))?;
        File::options()
            .write(true)
            .open(&lib_rs_file)
            .with_context(|| format!("couldn't open {}", lib_rs_file.display()))?
            .write(prettyplease::unparse(&file).as_bytes())
            .with_context(|| format!("couldn't write {}", lib_rs_file.display()))?;
        let rustfmt_err_file = path_helper_base(&chip_dir, &["rustfmt.err.log"]);
        let form_err_file = path_helper_base(&chip_dir, &["form.err.log"]);
        if let Some(form_bin_path) = crate::FORM.get() {
            tracing::info!("Running form");

            // move the lib.rs file to src, then split with form.
            let new_lib_rs_file = path_helper_base(&chip_dir, &["lib.rs"]);
            std::fs::rename(lib_rs_file, &new_lib_rs_file)
                .with_context(|| "While moving lib.rs file")?;
            Command::new(form_bin_path)
                .arg("--input")
                .arg(&new_lib_rs_file)
                .arg("--outdir")
                .arg(&src_dir)
                .capture_outputs(
                    true,
                    "form",
                    None,
                    Some(&form_err_file),
                    &process_stderr_paths,
                )
                .with_context(|| "failed to form")?;
            std::fs::remove_file(&new_lib_rs_file)
                .with_context(|| "While removing lib.rs file after form")?;
        }
        if let Some(rustfmt_bin_path) = crate::RUSTFMT.get() {
            tracing::info!("Running rustfmt");
            // Run `rusfmt`, capturing stderr to a log file

            // find all .rs files in src_dir and it's subdirectories
            let mut src_files = vec![];
            visit_dirs(&src_dir, &mut |e: &fs::DirEntry| {
                if e.path().extension().unwrap_or_default() == "rs" {
                    src_files.push(e.path());
                }
            })
            .context("couldn't visit")?;
            src_files.sort();

            for entry in src_files {
                Command::new(rustfmt_bin_path)
                    .arg(entry)
                    .args(["--edition", "2021"])
                    .capture_outputs(
                        false,
                        "rustfmt",
                        None,
                        Some(&rustfmt_err_file),
                        &process_stderr_paths,
                    )
                    .with_context(|| "failed to format")?;
            }

            process_stderr_paths.push(rustfmt_err_file);
        }
        Ok((chip_dir, process_stderr_paths))
    }
}

fn visit_dirs(dir: &Path, cb: &mut dyn FnMut(&fs::DirEntry)) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, cb)?;
            } else {
                cb(&entry);
            }
        }
    }
    Ok(())
}
