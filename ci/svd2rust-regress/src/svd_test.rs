use errors::*;
use std::io::prelude::*;
use std::fs::{remove_dir_all, File, OpenOptions};
use std::process::{Command, Output};
use tests::TestCase;
use reqwest;
use std::path::PathBuf;

static CRATES_ALL: &[&str] = &["bare-metal = \"0.1.0\"", "vcell = \"0.1.0\""];
static CRATES_MSP430: &[&str] = &["msp430 = \"0.1.0\""];
static CRATES_CORTEX_M: &[&str] = &["cortex-m = \"0.4.0\"", "cortex-m-rt = \"0.3.0\""];
static CRATES_RISCV: &[&str] = &["riscv = \"0.1.4\"", "riscv-rt = \"0.1.3\""];
static PROFILE_ALL: &[&str] = &["[profile.dev]", "incremental = false"];

fn path_helper(input: &[&str]) -> PathBuf {
    input.iter().collect()
}

fn path_helper_base(base: &PathBuf, input: &[&str]) -> PathBuf {
    let mut path = base.clone();
    input.iter().for_each(|p| path.push(p));
    path
}

/// Create and write to file
fn file_helper(payload: &str, path: &PathBuf) -> Result<()> {
    let mut f = File::create(path).chain_err(|| format!("Failed to create {:?}", path))?;

    f.write_all(payload.as_bytes())
        .chain_err(|| format!("Failed to write to {:?}", path))?;

    Ok(())
}

trait CommandHelper {
    fn capture_outputs(
        &self,
        cant_fail: bool,
        name: &str,
        stdout: Option<&PathBuf>,
        stderr: Option<&PathBuf>,
    ) -> Result<()>;
}

impl CommandHelper for Output {
    fn capture_outputs(
        &self,
        cant_fail: bool,
        name: &str,
        stdout: Option<&PathBuf>,
        stderr: Option<&PathBuf>,
    ) -> Result<()> {
        if cant_fail && !self.status.success() {
            bail!(format!("Process Failed! - {}", name))
        }

        if let Some(out) = stdout {
            let out_payload = String::from_utf8_lossy(&self.stdout);
            file_helper(&out_payload, out)?;
        }

        if let Some(err) = stderr {
            let err_payload = String::from_utf8_lossy(&self.stderr);
            file_helper(&err_payload, err)?;
        }

        Ok(())
    }
}

pub fn test(t: &TestCase, bin_path: &PathBuf) -> Result<()> {
    let user = match ::std::env::var("USER") {
        Ok(val) => val,
        Err(_) => "rusttester".into(),
    };

    // Remove the existing chip directory, if it exists
    let chip_dir = path_helper(&["output", &t.name()]);
    let _ = remove_dir_all(&chip_dir);

    // Create a new cargo project. It is necesary to set the user, otherwise
    //   cargo init will not work (when running in a container with no user set)
    Command::new("cargo")
        .env("USER", user)
        .arg("init")
        .arg("--name")
        .arg(&t.name())
        .arg("--vcs")
        .arg("none")
        .arg(&chip_dir)
        .output()
        .chain_err(|| "Failed to cargo init")?
        .capture_outputs(true, "cargo init", None, None)?;

    // Add some crates to the Cargo.toml of our new project
    let svd_toml = path_helper_base(&chip_dir, &["Cargo.toml"]);
    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(svd_toml)
        .chain_err(|| "Failed to open Cargo.toml for appending")?;

    use tests::Architecture::*;
    let crates = CRATES_ALL
        .iter()
        .chain(match &t.arch {
            &CortexM => CRATES_CORTEX_M.iter(),
            &RiscV => CRATES_RISCV.iter(),
            &Msp430 => CRATES_MSP430.iter(),
        })
        .chain(PROFILE_ALL.iter());

    for c in crates {
        writeln!(file, "{}", c).chain_err(|| "Failed to append to file!")?;
    }

    // Download the SVD as specified in the URL
    // TODO: Check for existing svd files? `--no-cache` flag?
    let svd = reqwest::get(&t.svd_url())
        .chain_err(|| "Failed to get svd URL")?
        .text()
        .chain_err(|| "SVD is bad text")?;

    // Write SVD contents to file
    let chip_svd = format!("{}.svd", &t.chip);
    let svd_file = path_helper_base(&chip_dir, &[&chip_svd]);
    file_helper(&svd, &svd_file)?;

    // Generate the lib.rs from the SVD file using the specified `svd2rust` binary
    let lib_rs_file = path_helper_base(&chip_dir, &["src", "lib.rs"]);
    let svd2rust_err_file = path_helper_base(&chip_dir, &["svd2rust.err.log"]);
    let target = match &t.arch {
        &CortexM => "cortex-m",
        &Msp430 => "msp430",
        &RiscV => "riscv",
    };
    Command::new(bin_path)
        .arg("-i")
        .arg(&chip_svd)
        .arg("--target")
        .arg(target)
        .current_dir(&chip_dir)
        .output()
        .chain_err(|| "failed to execute process")?
        .capture_outputs(
            true,
            "svd2rust",
            Some(&lib_rs_file),
            Some(&svd2rust_err_file),
        )?;

    // TODO: rustfmt < 0.4.0 seems to mangle some doc comments. Omit until this is fixed
    // Run `cargo fmt`, capturing stderr to a log file
    // let cargo_fmt_err_file = path_helper_base(&chip_dir, &["cargo-fmt.err.log"]);
    // Command::new("cargo")
    //     .arg("fmt")
    //     .current_dir(&chip_dir)
    //     .output()
    //     .chain_err(|| "failed to format")?
    //     .capture_outputs(false, "cargo fmt", None, Some(&cargo_fmt_err_file))?;

    // Run `cargo check`, capturing stderr to a log file
    let cargo_check_err_file = path_helper_base(&chip_dir, &["cargo-check.err.log"]);
    Command::new("cargo")
        .arg("check")
        .current_dir(&chip_dir)
        .output()
        .chain_err(|| "failed to check")?
        .capture_outputs(true, "cargo check", None, Some(&cargo_check_err_file))?;

    Ok(())
}
