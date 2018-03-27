use std::path;
error_chain!{
    errors {
        ProcessFailed(command: String, stderr: Option<path::PathBuf>, stdout: Option<path::PathBuf>) {
            description("Process Failed")
            display("Process Failed - {}", command)
        }
    }
}
