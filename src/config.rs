use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Config {
    pub target: Target,
    pub atomics: bool,
    pub atomics_feature: Option<String>,
    pub generic_mod: bool,
    pub make_mod: bool,
    pub ignore_groups: bool,
    pub keep_list: bool,
    pub strict: bool,
    pub pascal_enum_values: bool,
    pub feature_group: bool,
    pub feature_peripheral: bool,
    pub max_cluster_size: bool,
    pub impl_debug: bool,
    pub impl_debug_feature: Option<String>,
    pub output_dir: Option<PathBuf>,
    pub input: Option<PathBuf>,
    pub source_type: SourceType,
    pub log_level: Option<String>,
    pub interrupt_link_section: Option<String>,
    pub reexport_core_peripherals: bool,
    pub reexport_interrupt: bool,
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
