use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Config {
    #[cfg_attr(feature = "serde", serde(default))]
    pub target: Target,
    #[cfg_attr(feature = "serde", serde(default))]
    pub atomics: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub atomics_feature: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub generic_mod: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub make_mod: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub ignore_groups: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub keep_list: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub strict: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub pascal_enum_values: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub feature_group: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub feature_peripheral: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub max_cluster_size: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub impl_debug: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub impl_debug_feature: Option<String>,
    #[cfg_attr(feature = "serde", serde(default = "current_dir"))]
    pub output_dir: PathBuf,
    #[cfg_attr(feature = "serde", serde(default))]
    pub input: Option<PathBuf>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub source_type: SourceType,
    #[cfg_attr(feature = "serde", serde(default))]
    pub log_level: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub interrupt_link_section: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub reexport_core_peripherals: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub reexport_interrupt: bool,
}

fn current_dir() -> PathBuf {
    PathBuf::from(".")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target: Target::default(),
            atomics: false,
            atomics_feature: None,
            generic_mod: false,
            make_mod: false,
            ignore_groups: false,
            keep_list: false,
            strict: false,
            pascal_enum_values: false,
            feature_group: false,
            feature_peripheral: false,
            max_cluster_size: false,
            impl_debug: false,
            impl_debug_feature: None,
            output_dir: current_dir(),
            input: None,
            source_type: SourceType::default(),
            log_level: None,
            interrupt_link_section: None,
            reexport_core_peripherals: false,
            reexport_interrupt: false,
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
#[allow(non_camel_case_types)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Target {
    #[cfg_attr(feature = "serde", serde(rename = "cortex-m"))]
    #[default]
    CortexM,
    #[cfg_attr(feature = "serde", serde(rename = "msp430"))]
    Msp430,
    #[cfg_attr(feature = "serde", serde(rename = "riscv"))]
    RISCV,
    #[cfg_attr(feature = "serde", serde(rename = "xtensa-lx"))]
    XtensaLX,
    #[cfg_attr(feature = "serde", serde(rename = "mips"))]
    Mips,
    #[cfg_attr(feature = "serde", serde(rename = "none"))]
    None,
}

impl Target {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "cortex-m" => Target::CortexM,
            "msp430" => Target::Msp430,
            "riscv" => Target::RISCV,
            "xtensa-lx" => Target::XtensaLX,
            "mips" => Target::Mips,
            "none" => Target::None,
            _ => bail!("unknown target {}", s),
        })
    }
}

#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SourceType {
    #[default]
    Xml,
    #[cfg(feature = "yaml")]
    Yaml,
    #[cfg(feature = "json")]
    Json,
}

impl SourceType {
    /// Make a new [`SourceType`] from a given extension.
    pub fn from_extension(s: &str) -> Option<Self> {
        match s {
            "svd" | "xml" => Some(Self::Xml),
            #[cfg(feature = "yaml")]
            "yml" | "yaml" => Some(Self::Yaml),
            #[cfg(feature = "json")]
            "json" => Some(Self::Json),
            _ => None,
        }
    }
    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
            .unwrap_or_default()
    }
}
