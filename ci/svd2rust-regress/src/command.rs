use std::process::Command;

use anyhow::Context;

pub trait CommandExt {
    #[track_caller]
    fn run(&mut self, hide: bool) -> Result<(), anyhow::Error>;

    #[track_caller]
    fn get_output(&mut self, can_fail: bool) -> Result<std::process::Output, anyhow::Error>;

    #[track_caller]
    fn get_output_string(&mut self) -> Result<String, anyhow::Error>;

    fn display(&self) -> String;
}

impl CommandExt for Command {
    #[track_caller]
    fn run(&mut self, hide: bool) -> Result<(), anyhow::Error> {
        if hide {
            self.stdout(std::process::Stdio::null())
                .stdin(std::process::Stdio::null());
        }
        let status = self
            .status()
            .with_context(|| format!("fail! {}", self.display()))?;
        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("command `{}` failed", self.display())
        }
    }

    #[track_caller]
    fn get_output(&mut self, can_fail: bool) -> Result<std::process::Output, anyhow::Error> {
        let output = self
            .output()
            .with_context(|| format!("command `{}` couldn't be run", self.display()))?;
        if output.status.success() || can_fail {
            Ok(output)
        } else {
            anyhow::bail!(
                "command `{}` failed: stdout: {}\nstderr: {}",
                self.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )
        }
    }

    #[track_caller]
    fn get_output_string(&mut self) -> Result<String, anyhow::Error> {
        String::from_utf8(self.get_output(true)?.stdout).map_err(Into::into)
    }

    fn display(&self) -> String {
        format!(
            "{}{} {}",
            self.get_current_dir()
                .map(|d| format!("{} ", d.display()))
                .unwrap_or_default(),
            self.get_program().to_string_lossy(),
            self.get_args()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}
