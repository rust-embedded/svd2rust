use self::RunWhen::*;
use anyhow::Context;
pub use svd2rust::util::Target;
use svd2rust::util::ToSanitizedCase;

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
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
    Espressif,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum RunWhen {
    Always,
    NotShort,

    // TODO: Never doesn't really do anything right now
    Never,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct TestCase {
    pub arch: Target,
    pub mfgr: Manufacturer,
    pub chip: String,
    svd_url: Option<String>,
    pub should_pass: bool,
    run_when: RunWhen,
}

impl TestCase {
    pub fn svd_url(&self) -> String {
        match &self.svd_url {
            Some(u) => u.to_owned(),
            None => format!("https://raw.githubusercontent.com/posborne/cmsis-svd/master/data/{vendor:?}/{chip}.svd",
                  vendor = self.mfgr,
                  chip = self.chip
            )
        }
    }

    pub fn should_run(&self, short_test: bool) -> bool {
        match (&self.run_when, short_test) {
            (&Always, _) => true,
            (&NotShort, true) => false,
            (_, _) => true,
        }
    }

    pub fn name(&self) -> String {
        format!("{:?}-{}", self.mfgr, self.chip.replace('.', "_"))
            .to_sanitized_snake_case()
            .into()
    }
}

pub fn tests(opts: Option<&crate::Opts>) -> Result<&'static [TestCase], anyhow::Error> {
    pub static TESTS: std::sync::OnceLock<Vec<TestCase>> = std::sync::OnceLock::new();

    if let Some(cases) = TESTS.get() {
        Ok(cases)
    } else {
        let path = opts
            .map(|o| o.test_cases.clone())
            .ok_or_else(|| anyhow::format_err!("no test cases specified"))?;
        let cases: Vec<TestCase> = serde_json::from_reader(
            std::fs::OpenOptions::new()
                .read(true)
                .open(&path)
                .with_context(|| format!("couldn't open file {}", path.display()))?,
        )?;
        Ok(TESTS.get_or_init(|| cases))
    }
}
