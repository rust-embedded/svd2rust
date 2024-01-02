use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Config {
    pub target: Target,
    pub atomics: bool,
    pub atomics_feature: Option<String>,
    pub generic_mod: bool,
    pub make_mod: bool,
    pub skip_crate_attributes: bool,
    pub ignore_groups: bool,
    pub keep_list: bool,
    pub strict: bool,
    pub pascal_enum_values: bool,
    pub feature_group: bool,
    pub feature_peripheral: bool,
    pub max_cluster_size: bool,
    pub impl_debug: bool,
    pub impl_debug_feature: Option<String>,
    pub impl_defmt: Option<String>,
    pub output_dir: Option<PathBuf>,
    pub input: Option<PathBuf>,
    pub source_type: SourceType,
    pub log_level: Option<String>,
    pub interrupt_link_section: Option<String>,
    pub reexport_core_peripherals: bool,
    pub reexport_interrupt: bool,
    pub ident_formats: IdentFormats,
}

#[allow(clippy::upper_case_acronyms)]
#[allow(non_camel_case_types)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Target::CortexM => "cortex-m",
            Target::Msp430 => "msp430",
            Target::RISCV => "riscv",
            Target::XtensaLX => "xtensa-lx",
            Target::Mips => "mips",
            Target::None => "none",
        })
    }
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

    pub const fn all() -> &'static [Target] {
        use self::Target::*;
        &[CortexM, Msp430, RISCV, XtensaLX, Mips]
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

#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Case {
    #[default]
    Constant,
    Pascal,
    Snake,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
pub struct IdentFormat {
    pub case: Option<Case>,
    pub prefix: String,
    pub suffix: String,
}

impl IdentFormat {
    pub fn case(mut self, case: Case) -> Self {
        self.case = Some(case);
        self
    }
    pub fn constant_case(mut self) -> Self {
        self.case = Some(Case::Constant);
        self
    }
    pub fn pascal_case(mut self) -> Self {
        self.case = Some(Case::Pascal);
        self
    }
    pub fn scake_case(mut self) -> Self {
        self.case = Some(Case::Pascal);
        self
    }
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = prefix.into();
        self
    }
    pub fn suffix(mut self, suffix: &str) -> Self {
        self.suffix = suffix.into();
        self
    }
    pub fn parse(s: &str) -> Result<Self, ()> {
        let mut it = s.split(":");
        match (it.next(), it.next(), it.next(), it.next()) {
            (Some(prefix), Some(case), Some(suffix), None) => {
                let case = match case {
                    "C" | "CONSTANT" => Some(Case::Constant),
                    "P" | "Pascal" => Some(Case::Pascal),
                    "S" | "snake" => Some(Case::Snake),
                    "_" => None,
                    _ => return Err(()),
                };
                Ok(Self {
                    case,
                    prefix: prefix.into(),
                    suffix: suffix.into(),
                })
            }
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
pub struct IdentFormats {
    pub field_reader: IdentFormat,
    pub field_writer: IdentFormat,
    pub enum_name: IdentFormat,
    pub enum_write_name: IdentFormat,
    pub enum_value: IdentFormat,
    pub interrupt: IdentFormat,
    pub cluster: IdentFormat,
    pub register: IdentFormat,
    pub register_spec: IdentFormat,
    pub peripheral: IdentFormat,
}

impl Default for IdentFormats {
    fn default() -> Self {
        Self {
            field_reader: IdentFormat::default().constant_case().suffix("_R"),
            field_writer: IdentFormat::default().constant_case().suffix("_W"),
            enum_name: IdentFormat::default().constant_case().suffix("_A"),
            enum_write_name: IdentFormat::default().constant_case().suffix("_AW"),
            enum_value: IdentFormat::default().constant_case(),
            interrupt: IdentFormat::default().constant_case(),
            cluster: IdentFormat::default().constant_case(),
            register: IdentFormat::default().constant_case(),
            register_spec: IdentFormat::default().constant_case().suffix("_SPEC"),
            peripheral: IdentFormat::default().constant_case(),
        }
    }
}
