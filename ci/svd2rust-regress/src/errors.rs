use std::path::PathBuf;
error_chain!{
    errors {
        ProcessFailed(command: String, stderr: Option<PathBuf>, stdout: Option<PathBuf>) {
            description("Process Failed")
            display("Process Failed - {}", command)
        }
    }
}
