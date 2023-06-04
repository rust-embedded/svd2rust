use anyhow::{Context, Result};
use svd2rust::Target;

use crate::tests::TestCase;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};

const CRATES_ALL: &[&str] = &["critical-section = \"1.0\"", "vcell = \"0.1.2\""];
const CRATES_MSP430: &[&str] = &["msp430 = \"0.4.0\"", "msp430-rt = \"0.4.0\""];
const CRATES_ATOMICS: &[&str] =
    &["portable-atomic = { version = \"0.3.16\", default-features = false }"];
const CRATES_CORTEX_M: &[&str] = &["cortex-m = \"0.7.6\"", "cortex-m-rt = \"0.6.13\""];
const CRATES_RISCV: &[&str] = &["riscv = \"0.9.0\"", "riscv-rt = \"0.9.0\""];
const CRATES_XTENSALX: &[&str] = &["xtensa-lx-rt = \"0.9.0\"", "xtensa-lx = \"0.6.0\""];
const CRATES_MIPS: &[&str] = &["mips-mcu = \"0.1.0\""];
const PROFILE_ALL: &[&str] = &["[profile.dev]", "incremental = false"];
const FEATURES_ALL: &[&str] = &["[features]"];
const FEATURES_XTENSALX: &[&str] = &["default = [\"xtensa-lx/esp32\", \"xtensa-lx-rt/esp32\"]"];

fn path_helper(input: &[&str]) -> PathBuf {
    input.iter().collect()
}

fn path_helper_base(base: &Path, input: &[&str]) -> PathBuf {
    input
        .iter()
        .fold(base.to_owned(), |b: PathBuf, p| b.join(p))
}

/// Create and write to file
fn file_helper(payload: &str, path: &Path) -> Result<()> {
    let mut f = File::create(path).with_context(|| format!("Failed to create {path:?}"))?;

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
    #[error(transparent)]
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
        &self,
        cant_fail: bool,
        name: &str,
        stdout: Option<&PathBuf>,
        stderr: Option<&PathBuf>,
        previous_processes_stderr: &[PathBuf],
    ) -> Result<(), TestError>;
}

impl CommandHelper for Output {
    fn capture_outputs(
        &self,
        cant_fail: bool,
        name: &str,
        stdout: Option<&PathBuf>,
        stderr: Option<&PathBuf>,
        previous_processes_stderr: &[PathBuf],
    ) -> Result<(), TestError> {
        if let Some(out) = stdout {
            let out_payload = String::from_utf8_lossy(&self.stdout);
            file_helper(&out_payload, out)?;
        };

        if let Some(err) = stderr {
            let err_payload = String::from_utf8_lossy(&self.stderr);
            file_helper(&err_payload, err)?;
        };

        if cant_fail && !self.status.success() {
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
    pub fn test(
        &self,
        bin_path: &PathBuf,
        rustfmt_bin_path: Option<&PathBuf>,
        atomics: bool,
        verbosity: u8,
    ) -> Result<Option<Vec<PathBuf>>, TestError> {
        let (chip_dir, mut process_stderr_paths) =
            self.setup_case(atomics, bin_path, rustfmt_bin_path)?;
        // Run `cargo check`, capturing stderr to a log file
        let cargo_check_err_file = path_helper_base(&chip_dir, &["cargo-check.err.log"]);
        let output = Command::new("cargo")
            .arg("check")
            .current_dir(&chip_dir)
            .output()
            .with_context(|| "failed to check")?;
        output.capture_outputs(
            true,
            "cargo check",
            None,
            Some(&cargo_check_err_file),
            &process_stderr_paths,
        )?;
        process_stderr_paths.push(cargo_check_err_file);
        Ok(if verbosity > 1 {
            Some(process_stderr_paths)
        } else {
            None
        })
    }

    pub fn setup_case(
        &self,
        atomics: bool,
        bin_path: &PathBuf,
        rustfmt_bin_path: Option<&PathBuf>,
    ) -> Result<(PathBuf, Vec<PathBuf>), TestError> {
        let user = match std::env::var("USER") {
            Ok(val) => val,
            Err(_) => "rusttester".into(),
        };
        let chip_dir = path_helper(&["output", &self.name()]);
        if let Err(err) = fs::remove_dir_all(&chip_dir) {
            match err.kind() {
                std::io::ErrorKind::NotFound => (),
                _ => Err(err).with_context(|| "While removing chip directory")?,
            }
        }
        let mut process_stderr_paths: Vec<PathBuf> = vec![];
        Command::new("cargo")
            .env("USER", user)
            .arg("init")
            .arg("--name")
            .arg(&self.name())
            .arg("--vcs")
            .arg("none")
            .arg(&chip_dir)
            .output()
            .with_context(|| "Failed to cargo init")?
            .capture_outputs(true, "cargo init", None, None, &[])?;
        let svd_toml = path_helper_base(&chip_dir, &["Cargo.toml"]);
        let mut file = OpenOptions::new()
            .write(true)
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
            .chain(if atomics {
                CRATES_ATOMICS.iter()
            } else {
                [].iter()
            })
            .chain(PROFILE_ALL.iter())
            .chain(FEATURES_ALL.iter())
            .chain(match &self.arch {
                Target::XtensaLX => FEATURES_XTENSALX.iter(),
                _ => [].iter(),
            });
        for c in crates {
            writeln!(file, "{}", c).with_context(|| "Failed to append to file!")?;
        }
        let svd = reqwest::blocking::get(self.svd_url())
            .with_context(|| "Failed to get svd URL")?
            .text()
            .with_context(|| "SVD is bad text")?;
        let chip_svd = format!("{}.svd", &self.chip);
        let svd_file = path_helper_base(&chip_dir, &[&chip_svd]);
        file_helper(&svd, &svd_file)?;
        let lib_rs_file = path_helper_base(&chip_dir, &["src", "lib.rs"]);
        let svd2rust_err_file = path_helper_base(&chip_dir, &["svd2rust.err.log"]);
        let target = match self.arch {
            Target::CortexM => "cortex-m",
            Target::Msp430 => "msp430",
            Target::Mips => "mips",
            Target::RISCV => "riscv",
            Target::XtensaLX => "xtensa-lx",
            Target::None => unreachable!(),
        };
        let mut svd2rust_bin = Command::new(bin_path);
        if atomics {
            svd2rust_bin.arg("--atomics");
        }
        let output = svd2rust_bin
            .args(["-i", &chip_svd])
            .args(["--target", target])
            .current_dir(&chip_dir)
            .output()
            .with_context(|| "failed to execute process")?;
        output.capture_outputs(
            true,
            "svd2rust",
            Some(&lib_rs_file).filter(|_| {
                (self.arch != Target::CortexM)
                    && (self.arch != Target::Msp430)
                    && (self.arch != Target::XtensaLX)
            }),
            Some(&svd2rust_err_file),
            &[],
        )?;
        process_stderr_paths.push(svd2rust_err_file);
        match self.arch {
            Target::CortexM | Target::Mips | Target::Msp430 | Target::XtensaLX => {
                // TODO: Give error the path to stderr
                fs::rename(path_helper_base(&chip_dir, &["lib.rs"]), &lib_rs_file)
                    .with_context(|| "While moving lib.rs file")?
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
        if let Some(rustfmt_bin_path) = rustfmt_bin_path {
            // Run `cargo fmt`, capturing stderr to a log file

            let output = Command::new(rustfmt_bin_path)
                .arg(lib_rs_file)
                .output()
                .with_context(|| "failed to format")?;
            output.capture_outputs(
                false,
                "rustfmt",
                None,
                Some(&rustfmt_err_file),
                &process_stderr_paths,
            )?;
            process_stderr_paths.push(rustfmt_err_file);
        }
        Ok((chip_dir, process_stderr_paths))
    }
}
