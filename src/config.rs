use anyhow::{bail, Result};
use proc_macro2::{Span, TokenStream};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    str::FromStr,
};
use syn::{punctuated::Punctuated, Ident};

use crate::util::path_segment;

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
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
    pub ident_formats_theme: Option<IdentFormatsTheme>,
    pub field_names_for_enums: bool,
    pub base_address_shift: u64,
    /// Path to YAML file with chip-specific settings
    pub settings_file: Option<PathBuf>,
    /// Chip-specific settings
    pub settings: Settings,
}

impl Config {
    pub fn extra_build(&self) -> Option<TokenStream> {
        self.settings.extra_build()
    }
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
    #[cfg_attr(feature = "serde", serde(rename = "avr"))]
    Avr,
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
            Target::Avr => "avr",
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
            "avr" => Target::Avr,
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
#[non_exhaustive]
pub enum Case {
    #[default]
    Constant,
    Pascal,
    Snake,
}

impl Case {
    pub fn parse(c: &str) -> Result<Option<Self>, IdentFormatError> {
        Ok(match c {
            "" | "unchanged" | "svd" => None,
            "p" | "pascal" | "type" => Some(Case::Pascal),
            "s" | "snake" | "lower" => Some(Case::Snake),
            "c" | "constant" | "upper" => Some(Case::Constant),
            _ => {
                return Err(IdentFormatError::UnknownCase(c.into()));
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentFormatError {
    UnknownCase(String),
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
pub struct IdentFormat {
    // Ident case. `None` means don't change
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
    pub fn snake_case(mut self) -> Self {
        self.case = Some(Case::Snake);
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
    pub fn parse(s: &str) -> Result<Self, IdentFormatError> {
        let mut f = s.split(':');
        match (f.next(), f.next(), f.next()) {
            (Some(p), Some(c), Some(s)) => {
                let case = Case::parse(c)?;
                Ok(Self {
                    case,
                    prefix: p.into(),
                    suffix: s.into(),
                })
            }
            (Some(p), Some(c), None) => {
                let case = Case::parse(c)?;
                Ok(Self {
                    case,
                    prefix: p.into(),
                    suffix: "".into(),
                })
            }
            (Some(c), None, None) => {
                let case = Case::parse(c)?;
                Ok(Self {
                    case,
                    prefix: "".into(),
                    suffix: "".into(),
                })
            }
            _ => Err(IdentFormatError::Other),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
pub struct IdentFormats(HashMap<String, IdentFormat>);

impl IdentFormats {
    fn common() -> Self {
        let snake = IdentFormat::default().snake_case();
        Self(HashMap::from([
            ("field_accessor".into(), snake.clone()),
            ("register_accessor".into(), snake.clone()),
            ("enum_value_accessor".into(), snake.clone()),
            ("cluster_accessor".into(), snake.clone()),
            ("register_mod".into(), snake.clone()),
            ("cluster_mod".into(), snake.clone()),
            ("peripheral_mod".into(), snake.clone()),
            ("peripheral_feature".into(), snake),
        ]))
    }

    pub fn default_theme() -> Self {
        let mut map = Self::common();

        let pascal = IdentFormat::default().pascal_case();
        map.extend([
            ("field_reader".into(), pascal.clone().suffix("R")),
            ("field_writer".into(), pascal.clone().suffix("W")),
            ("enum_name".into(), pascal.clone()),
            ("enum_read_name".into(), pascal.clone()),
            ("enum_write_name".into(), pascal.clone().suffix("WO")),
            ("enum_value".into(), pascal.clone()),
            ("interrupt".into(), IdentFormat::default()),
            ("register".into(), pascal.clone()),
            ("cluster".into(), pascal.clone()),
            ("register_spec".into(), pascal.clone().suffix("Spec")),
            ("peripheral".into(), pascal),
            (
                "peripheral_singleton".into(),
                IdentFormat::default().snake_case(),
            ),
        ]);

        map
    }
    pub fn legacy_theme() -> Self {
        let mut map = Self::common();

        let constant = IdentFormat::default().constant_case();
        map.extend([
            ("field_reader".into(), constant.clone().suffix("_R")),
            ("field_writer".into(), constant.clone().suffix("_W")),
            ("enum_name".into(), constant.clone().suffix("_A")),
            ("enum_read_name".into(), constant.clone().suffix("_A")),
            ("enum_write_name".into(), constant.clone().suffix("_AW")),
            ("enum_value".into(), constant.clone()),
            ("interrupt".into(), constant.clone()),
            ("cluster".into(), constant.clone()),
            ("register".into(), constant.clone()),
            ("register_spec".into(), constant.clone().suffix("_SPEC")),
            ("peripheral".into(), constant.clone()),
            ("peripheral_singleton".into(), constant),
        ]);

        map
    }
}

impl Deref for IdentFormats {
    type Target = HashMap<String, IdentFormat>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for IdentFormats {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentFormatsTheme {
    Legacy,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
/// Chip-specific settings
pub struct Settings {
    /// Path to chip HTML generated by svdtools
    pub html_url: Option<url::Url>,
    pub crate_path: Option<CratePath>,
    /// RISC-V specific settings
    pub riscv_config: Option<riscv::RiscvConfig>,
}

impl Settings {
    pub fn update_from(&mut self, source: Self) {
        if source.html_url.is_some() {
            self.html_url = source.html_url;
        }
        if source.crate_path.is_some() {
            self.crate_path = source.crate_path;
        }
        if source.riscv_config.is_some() {
            self.riscv_config = source.riscv_config;
        }
    }

    pub fn extra_build(&self) -> Option<TokenStream> {
        self.riscv_config.as_ref().and_then(|cfg| cfg.extra_build())
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CratePath(pub syn::Path);

impl Default for CratePath {
    fn default() -> Self {
        let mut segments = Punctuated::new();
        segments.push(path_segment(Ident::new("crate", Span::call_site())));
        Self(syn::Path {
            leading_colon: None,
            segments,
        })
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for CratePath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str(&s).unwrap())
    }
}

impl FromStr for CratePath {
    type Err = syn::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        syn::parse_str(s).map(Self)
    }
}

pub mod riscv;
