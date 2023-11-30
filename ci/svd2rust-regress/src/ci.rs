use std::path::Path;

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

impl Ci {
    pub fn run(
        &self,
        opts: &Opts,

    ) -> Result<(), anyhow::Error> {
        let command = 'command: {
            // FIXME: this is just fun rust, probably not idiomatic.
            for line in self.comment.lines() {
                let Some(command) = line.strip_prefix("/ci diff ") else {
                    continue;
                };
                break 'command command;
            }
            std::process::exit(0);
        };
        todo!()
    }
}
