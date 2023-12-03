use std::process::Command;
use std::{ffi::OsStr, path::Path};
use std::{iter::IntoIterator, path::PathBuf};

use anyhow::Context;

use crate::command::CommandExt;

pub fn run_gh<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("gh");
    command.args(args);
    command
}

pub fn get_current_pr() -> Result<(usize, String), anyhow::Error> {
    #[derive(serde::Deserialize)]
    struct Pr {
        number: usize,
        #[serde(rename = "headRefOid")]
        head_ref_oid: String,
    }
    let pr = run_gh(["pr", "view", "--json", "headRefOid,number"]).get_output_string()?;
    let Pr {
        number,
        head_ref_oid,
    } = serde_json::from_str(&pr)?;

    Ok((number, head_ref_oid))
}

pub fn get_sha_run_id(sha: &str) -> Result<usize, anyhow::Error> {
    let run_id = run_gh([
        "api",
        &format!("repos/:owner/:repo/actions/runs?event=pull_request&head_sha={sha}"),
        "--jq",
        r#"[.workflow_runs[] | select(.name == "Continuous integration")][0] | .id"#,
    ])
    .get_output_string()?;
    if run_id.trim().is_empty() {
        anyhow::bail!("no run id found for sha `{}`", sha);
    }
    run_id
        .trim()
        .parse()
        .with_context(|| anyhow::anyhow!("couldn't parse api output: {run_id}"))
}

pub fn get_release_run_id(event: &str) -> Result<usize, anyhow::Error> {
    let query = match event {
        "master" => "branch=master".to_owned(),
        _ => anyhow::bail!("unknown event"),
    };
    let run_id = run_gh([
        "api",
        &format!("repos/:owner/:repo/actions/runs?{query}"),
        "--jq",
        r#"[.workflow_runs[] | select(.name == "release")][0] | .id"#,
    ])
    .get_output_string()?;
    run_id.trim().parse().map_err(Into::into)
}

fn find_executable(dir: &Path, begins: &str) -> Result<Option<PathBuf>, anyhow::Error> {
    let find = |entry, begins: &str| -> Result<Option<PathBuf>, std::io::Error> {
        let entry: std::fs::DirEntry = entry?;
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        if entry.metadata()?.is_file()
            && filename.starts_with(begins)
            && entry
                .path()
                .extension()
                .is_some_and(|s| s == std::env::consts::EXE_EXTENSION)
            && !entry.path().extension().is_some_and(|s| s == "gz")
        {
            Ok(Some(entry.path()))
        } else {
            Ok(None)
        }
    };
    let mut read_dir = std::fs::read_dir(dir)?;
    read_dir
        .find_map(|entry| find(entry, begins).transpose())
        .transpose()
        .map_err(Into::into)
}

pub fn get_release_binary_artifact(
    reference: &str,
    output_dir: &Path,
) -> Result<PathBuf, anyhow::Error> {
    let output_dir = output_dir.join(".binary").join(reference);
    if let Some(binary) = find_executable(&output_dir, "svd2rust").unwrap_or_default() {
        return Ok(binary);
    }

    match reference {
        reference if reference.starts_with('v') || matches!(reference, "master" | "latest") => {
            let tag = if reference == "master" {
                Some("Unreleased")
            } else if reference == "latest" {
                None
            } else {
                Some(reference)
            };

            let artifact = if cfg!(target_os = "linux") {
                "svd2rust-x86_64-unknown-linux-gnu.gz"
            } else if cfg!(windows) {
                "svd2rust-x86_64-pc-windows-msvc.exe"
            } else if cfg!(macos) && cfg!(target_arch = "x86_64") {
                "svd2rust-x86_64-apple-darwin.gz"
            } else if cfg!(macos) && cfg!(target_arch = "aarch64") {
                "svd2rust-aarch64-apple-darwin.gz"
            } else {
                anyhow::bail!("regress with release artifact doesn't support current platform")
            };

            std::fs::remove_file(output_dir.join(artifact)).ok();

            run_gh(["release", "download", "--pattern", artifact, "--dir"])
                .arg(&output_dir)
                .args(tag)
                .run(true)?;

            if cfg!(target_os = "linux") || cfg!(macos) {
                Command::new("gzip")
                    .arg("-d")
                    .arg(output_dir.join(artifact))
                    .get_output()
                    .with_context(|| {
                        format!("couldn't gzip {}", output_dir.join(artifact).display())
                    })?;
            }
        }
        _ => {
            let run_id =
                get_release_run_id(reference).with_context(|| "couldn't get release run id")?;
            run_gh([
                "run",
                "download",
                &run_id.to_string(),
                "-n",
                "svd2rust-x86_64-unknown-linux-gnu",
                "--dir",
            ])
            .arg(&output_dir)
            .run(true)?;
        }
    }
    let binary =
        find_executable(&output_dir, "svd2rust").with_context(|| "couldn't find svd2rust")?;
    binary.ok_or_else(|| anyhow::anyhow!("no binary found"))
}

pub fn get_pr_binary_artifact(
    pr: usize,
    sha: &str,
    output_dir: &Path,
) -> Result<PathBuf, anyhow::Error> {
    let output_dir = output_dir.join(".binary").join(pr.to_string()).join(sha);

    if let Some(binary) = find_executable(&output_dir, "svd2rust").unwrap_or_default() {
        return Ok(binary);
    }

    let target = if cfg!(linux) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(macos) {
        "x86_64-apple-darwin"
    } else {
        anyhow::bail!("regress with pr artifact doesn't support current platform");
    };

    let run_id = get_sha_run_id(sha).context("when getting run id")?;
    run_gh([
        "run",
        "download",
        &run_id.to_string(),
        "-n",
        &format!("artifact-svd2rust-{}", target),
        "--dir",
    ])
    .arg(&output_dir)
    .run(true)?;

    let binary =
        find_executable(&output_dir, "svd2rust").with_context(|| "couldn't find svd2rust")?;
    binary.ok_or_else(|| anyhow::anyhow!("no binary found"))
}
