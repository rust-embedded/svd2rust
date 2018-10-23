pub fn generate_skeleton(features: Vec<String>) -> String {
    format!(
        "# THIS TOML FILE WAS GENERATED AND IS INCOMPLETE!
# Auto generated feature flags
default = [ {} ]
{}",
        features.join(", "),
        features.iter().fold(String::new(), |mut s, f| {
            s.push_str(&format!("{} = []\n", f));
            s
        })
    )
}
