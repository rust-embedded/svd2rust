pub fn generate_skeleton(features: Vec<String>) -> String {
    format!(
        "# THIS TOML FILE WAS GENERATED AND IS INCOMPLETE!
[dependencies]
bare-metal = \"0.2.0\"
cortex-m = \"0.5.0\"
vcell = \"0.1.0\"

[dependencies.cortex-m-rt]
optional = true
version = \"0.5.0\"

[features]
rt = [\"cortex-m-rt/device\"]

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
