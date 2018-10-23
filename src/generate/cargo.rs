pub fn generate_skeleton(features: Vec<String>) -> String {
    format!(
        "[features]
# Auto generated feature flags
# By default, all peripherals are enabled for use. To speed build times, select
# `--no-default-features`, and re-enable peripherals necessary for your use
default = [ {} ]

# Individual Peripherals
{}",
        features.join(", "),
        features.iter().fold(String::new(), |mut s, f| {
            s.push_str(&format!("{} = []\n", f));
            s
        })
    )
}
