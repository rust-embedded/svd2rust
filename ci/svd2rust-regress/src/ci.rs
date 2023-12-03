use crate::Opts;

#[derive(clap::Parser, Debug)]
#[clap(name = "continuous-integration")]
pub struct Ci {
    #[clap(long)]
    pub format: bool,
    /// Enable splitting `lib.rs` with `form`
    #[clap(long)]
    pub form_lib: bool,
    #[clap(env = "GITHUB_COMMENT")]
    pub comment: String,
    #[clap(env = "GITHUB_COMMENT_USER")]
    pub comment_user: String,
    #[clap(env = "GITHUB_COMMENT_PR")]
    pub comment_pr: String,
}

#[derive(serde::Serialize)]
struct Diff {
    command: String,
    needs_semver_checks: bool,
}

impl Ci {
    pub fn run(&self, _opts: &Opts) -> Result<(), anyhow::Error> {
        let mut diffs = vec![];
        for line in self.comment.lines() {
            let Some(command) = line.strip_prefix("/ci diff ") else {
                continue;
            };

            diffs.push(Diff {
                needs_semver_checks: command.contains("semver"),
                command: command.to_owned(),
            });
        }
        let json = serde_json::to_string(&diffs)?;
        crate::gha_print(&json);
        crate::gha_output("diffs", &json)?;
        Ok(())
    }
}
