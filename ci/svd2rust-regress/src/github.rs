use std::process::{Command, Output};
use std::{ffi::OsStr, path::Path};
use std::{iter::IntoIterator, path::PathBuf};

use anyhow::Context;

pub fn run_gh<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("gh");
    command.args(args);
    command
}

pub fn get_current_pr() -> Result<usize, anyhow::Error> {
    let pr = run_gh([
        "pr",
        "view",
        "--json",
        "number",
        "--template",
        "{{.number}}",
    ])
    .output()?;
    String::from_utf8(pr.stdout)?
        .trim()
        .parse()
        .map_err(Into::into)
}

pub fn get_pr_run_id(pr: usize) -> Result<usize, anyhow::Error> {
    let run_id = run_gh([
        "api",
        &format!("repos/:owner/:repo/actions/runs?event=pull_request&pr={pr}"),
        "--jq",
        r#"[.workflow_runs[] | select(.name == "Continuous integration")][0] | .id"#,
    ])
    .output()?;
    String::from_utf8(run_id.stdout)?
        .trim()
        .parse()
        .map_err(Into::into)
}

pub fn get_release_run_id(event: &str) -> Result<usize, anyhow::Error> {
    let query = match event {
        "master" => "branch=master".to_owned(),
        _ => anyhow::bail!("unknown event"),
    };
    let run_id = dbg!(run_gh([
        "api",
        &format!("repos/:owner/:repo/actions/runs?{query}"),
        "--jq",
        r#"[.workflow_runs[] | select(.name == "release")][0] | .id"#,
    ])
    .output())
    .with_context(|| "couldn't run gh")?;
    String::from_utf8(run_id.stdout)?
        .trim()
        .parse()
        .map_err(Into::into)
}

fn find(dir: &Path, begins: &str) -> Result<Option<PathBuf>, anyhow::Error> {
    let find = |entry, begins: &str| -> Result<Option<PathBuf>, std::io::Error> {
        let entry: std::fs::DirEntry = entry?;
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        if entry.metadata()?.is_file() && filename.starts_with(begins) {
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
    let output_dir = output_dir.join(reference);
    if let Some(binary) = find(&output_dir, "svd2rust")? {
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
            run_gh([
                "release",
                "download",
                "--pattern",
                "svd2rust-x86_64-unknown-linux-gnu.gz",
                "--dir",
            ])
            .arg(&output_dir)
            .args(tag)
            .status()?;

            Command::new("tar")
                .arg("-xzf")
                .arg(output_dir.join("svd2rust-x86_64-unknown-linux-gnu.gz"))
                .arg("-C")
                .arg(&output_dir)
                .output()
                .expect("Failed to execute command");
        }
        _ => {
            let run_id = get_release_run_id(reference)?;
            run_gh([
                "run",
                "download",
                &run_id.to_string(),
                "-n",
                "svd2rust-x86_64-unknown-linux-gnu",
                "--dir",
            ])
            .arg(&output_dir)
            .output()?;
        }
    }
    let binary = find(&output_dir, "svd2rust")?;
    binary.ok_or_else(|| anyhow::anyhow!("no binary found"))
}

pub fn get_pr_binary_artifact(pr: usize, output_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    let output_dir = output_dir.join(format!("{pr}"));
    let run_id = get_pr_run_id(pr)?;
    run_gh([
        "run",
        "download",
        &run_id.to_string(),
        "-n",
        "artifact-svd2rust-x86_64-unknown-linux-gnu",
        "--dir",
    ])
    .arg(&output_dir)
    .output()?;
    let mut read_dir = std::fs::read_dir(output_dir)?;
    let binary = read_dir
        .find_map(|entry| {
            let find = |entry| -> Result<Option<PathBuf>, std::io::Error> {
                let entry: std::fs::DirEntry = entry?;
                let filename = entry.file_name();
                let filename = filename.to_string_lossy();
                if entry.metadata()?.is_file() && filename.starts_with("svd2rust-regress") {
                    Ok(Some(entry.path()))
                } else {
                    Ok(None)
                }
            };
            find(entry).transpose()
        })
        .transpose()?;
    binary.ok_or_else(|| anyhow::anyhow!("no binary found"))
}
