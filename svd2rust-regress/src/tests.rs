use self::RunWhen::*;
use anyhow::Context;
use serde::Serialize as _;
pub use svd2rust::Target;

#[allow(clippy::upper_case_acronyms)]
#[derive(
    Debug, serde::Serialize, serde::Deserialize, PartialOrd, Ord, PartialEq, Eq, Clone, Copy,
)]
pub enum Manufacturer {
    Atmel,
    Freescale,
    Fujitsu,
    Holtek,
    Microchip,
    Nordic,
    Nuvoton,
    NXP,
    SiliconLabs,
    Spansion,
    STMicro,
    Toshiba,
    SiFive,
    TexasInstruments,
    Vorago,
    Espressif,
    RaspberryPi,
    Renesas,
    Unknown,
}

impl Manufacturer {
    pub const fn all() -> &'static [Self] {
        use self::Manufacturer::*;
        &[
            Atmel,
            Freescale,
            Fujitsu,
            Holtek,
            Microchip,
            Nordic,
            Nuvoton,
            NXP,
            SiliconLabs,
            Spansion,
            Vorago,
            STMicro,
            Toshiba,
            SiFive,
            RaspberryPi,
            Renesas,
            TexasInstruments,
            Espressif,
        ]
    }
}

impl std::fmt::Display for Manufacturer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.serialize(f)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum RunWhen {
    #[default]
    Always,
    NotShort,

    // TODO: Never doesn't really do anything right now
    Never,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct TestCase {
    pub arch: Target,
    pub mfgr: Manufacturer,
    pub chip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opts: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svd_url: Option<String>,
    #[serde(default = "true_")]
    pub should_pass: bool,
    #[serde(default)]
    pub skip_check: bool,
    #[serde(default)]
    pub run_when: RunWhen,
}

fn true_() -> bool {
    true
}

impl TestCase {
    pub fn svd_url(&self) -> String {
        match &self.svd_url {
            Some(u) => u.to_owned(),
            None => format!("https://raw.githubusercontent.com/cmsis-svd/cmsis-svd-data/main/data/{vendor:?}/{chip}.svd",
                  vendor = self.mfgr,
                  chip = self.chip
            )
        }
    }

    pub const fn should_run(&self, short_test: bool) -> bool {
        match (&self.run_when, short_test) {
            (&Always, _) => true,
            (&NotShort, true) => false,
            (_, _) => true,
        }
    }

    pub fn name(&self) -> String {
        let mut base_name = format!("{:?}-{}", self.mfgr, self.chip.replace('.', "_"));
        if let Some(suffix) = &self.suffix {
            base_name.push('-');
            base_name.push_str(suffix);
        }
        base_name
    }
}

pub fn tests(test_cases: Option<&std::path::Path>) -> Result<&'static [TestCase], anyhow::Error> {
    pub static TESTS: std::sync::OnceLock<Vec<TestCase>> = std::sync::OnceLock::new();

    if let Some(cases) = TESTS.get() {
        Ok(cases)
    } else {
        let path = test_cases.ok_or_else(|| anyhow::format_err!("no test cases specified"))?;
        let cases: Vec<TestCase> = if path.extension() != Some(std::ffi::OsStr::new("yml")) {
            serde_json::from_reader(
                std::fs::OpenOptions::new()
                    .read(true)
                    .open(path)
                    .with_context(|| format!("couldn't open file {}", path.display()))?,
            )?
        } else if path.extension() != Some(std::ffi::OsStr::new("json")) {
            serde_yaml::from_reader(
                std::fs::OpenOptions::new()
                    .read(true)
                    .open(path)
                    .with_context(|| format!("couldn't open file {}", path.display()))?,
            )?
        } else {
            anyhow::bail!("unknown file extension for {}", path.display());
        };
        Ok(TESTS.get_or_init(|| cases))
    }
}
