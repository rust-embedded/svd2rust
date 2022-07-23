use std::borrow::Cow;

use crate::svd::{
    Access, Cluster, Device, Field, Register, RegisterCluster, RegisterInfo, RegisterProperties,
};
use inflections::Inflect;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::{quote, ToTokens};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use svd_rs::{MaybeArray, PeripheralInfo};
use syn::parse_str;

use anyhow::{anyhow, bail, Context, Result};

pub const BITS_PER_BYTE: u32 = 8;

/// List of chars that some vendors use in their peripheral/field names but
/// that are not valid in Rust ident
const BLACKLIST_CHARS: &[char] = &['(', ')', '[', ']', '/', ' ', '-'];

#[derive(Clone, PartialEq, Debug)]
pub struct Config {
    pub target: Target,
    pub nightly: bool,
    pub generic_mod: bool,
    pub make_mod: bool,
    pub const_generic: bool,
    pub ignore_groups: bool,
    pub keep_list: bool,
    pub strict: bool,
    pub pascal_enum_values: bool,
    pub derive_more: bool,
    pub feature_group: bool,
    pub output_dir: PathBuf,
    pub source_type: SourceType,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target: Target::default(),
            nightly: false,
            generic_mod: false,
            make_mod: false,
            const_generic: false,
            ignore_groups: false,
            keep_list: false,
            strict: false,
            pascal_enum_values: false,
            derive_more: false,
            feature_group: false,
            output_dir: PathBuf::from("."),
            source_type: SourceType::default(),
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    CortexM,
    Msp430,
    RISCV,
    XtensaLX,
    Mips,
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

impl Default for Target {
    fn default() -> Self {
        Self::CortexM
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SourceType {
    Xml,
    #[cfg(feature = "yaml")]
    Yaml,
    #[cfg(feature = "json")]
    Json,
}

impl Default for SourceType {
    fn default() -> Self {
        Self::Xml
    }
}

impl SourceType {
    /// Make a new [`Source`] from a given extension.
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

/// Convert self string into specific case without overlapping to svd2rust internal names
pub trait ToSanitizedCase {
    /// Convert self into PascalCase.
    ///
    /// Use on name of enumeration values.
    fn to_sanitized_pascal_case(&self) -> Cow<str>;
    /// Convert self into CONSTANT_CASE.
    ///
    /// Use on name of reader structs, writer structs and enumerations.
    fn to_sanitized_constant_case(&self) -> Cow<str>;
    /// Convert self into snake_case, must use only if the target is used with extra prefix or suffix.
    fn to_sanitized_not_keyword_snake_case(&self) -> Cow<str>; // snake_case
    /// Convert self into snake_case target and ensure target is not a Rust keyword.
    ///
    /// If the sanitized target is a Rust keyword, this function adds an underline `_`
    /// to it.
    ///
    /// Use on name of peripheral modules, register modules and field modules.
    fn to_sanitized_snake_case(&self) -> Cow<str> {
        let s = self.to_sanitized_not_keyword_snake_case();
        sanitize_keyword(s)
    }
}

impl ToSanitizedCase for str {
    fn to_sanitized_pascal_case(&self) -> Cow<str> {
        let s = self.replace(BLACKLIST_CHARS, "");

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s.to_pascal_case()))
            }
            _ => Cow::from(s.to_pascal_case()),
        }
    }
    fn to_sanitized_constant_case(&self) -> Cow<str> {
        let s = self.replace(BLACKLIST_CHARS, "");

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s.to_constant_case()))
            }
            _ => Cow::from(s.to_constant_case()),
        }
    }
    fn to_sanitized_not_keyword_snake_case(&self) -> Cow<str> {
        const INTERNALS: [&str; 4] = ["set_bit", "clear_bit", "bit", "bits"];

        let s = self.replace(BLACKLIST_CHARS, "");
        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                format!("_{}", s.to_snake_case()).into()
            }
            _ => {
                let s = Cow::from(s.to_snake_case());
                if INTERNALS.contains(&s.as_ref()) {
                    s + "_"
                } else {
                    s
                }
            }
        }
    }
}

pub fn sanitize_keyword(sc: Cow<str>) -> Cow<str> {
    const KEYWORDS: [&str; 55] = [
        "abstract", "alignof", "as", "async", "await", "become", "box", "break", "const",
        "continue", "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for",
        "if", "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "offsetof",
        "override", "priv", "proc", "pub", "pure", "ref", "return", "self", "sizeof", "static",
        "struct", "super", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
        "virtual", "where", "while", "yield",
    ];
    if KEYWORDS.contains(&sc.as_ref()) {
        sc + "_"
    } else {
        sc
    }
}

pub fn respace(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(r"\n", "\n")
}

pub fn escape_brackets(s: &str) -> String {
    s.split('[')
        .fold("".to_string(), |acc, x| {
            if acc.is_empty() {
                x.to_string()
            } else if acc.ends_with('\\') {
                acc + "[" + x
            } else {
                acc + "\\[" + x
            }
        })
        .split(']')
        .fold("".to_string(), |acc, x| {
            if acc.is_empty() {
                x.to_string()
            } else if acc.ends_with('\\') {
                acc + "]" + x
            } else {
                acc + "\\]" + x
            }
        })
}

pub fn name_of<T: FullName>(maybe_array: &MaybeArray<T>, ignore_group: bool) -> Cow<str> {
    match maybe_array {
        MaybeArray::Single(info) => info.fullname(ignore_group),
        MaybeArray::Array(info, _) => replace_suffix(&info.fullname(ignore_group), "").into(),
    }
}

pub fn replace_suffix(name: &str, suffix: &str) -> String {
    if name.contains("[%s]") {
        name.replace("[%s]", suffix)
    } else {
        name.replace("%s", suffix)
    }
}

pub fn access_of(properties: &RegisterProperties, fields: Option<&[Field]>) -> Access {
    properties.access.unwrap_or_else(|| {
        if let Some(fields) = fields {
            if fields.iter().all(|f| f.access == Some(Access::ReadOnly)) {
                Access::ReadOnly
            } else if fields.iter().all(|f| f.access == Some(Access::WriteOnce)) {
                Access::WriteOnce
            } else if fields
                .iter()
                .all(|f| f.access == Some(Access::ReadWriteOnce))
            {
                Access::ReadWriteOnce
            } else if fields
                .iter()
                .all(|f| f.access == Some(Access::WriteOnly) || f.access == Some(Access::WriteOnce))
            {
                Access::WriteOnly
            } else {
                Access::ReadWrite
            }
        } else {
            Access::ReadWrite
        }
    })
}

pub fn digit_or_hex(n: u64) -> TokenStream {
    if n < 10 {
        unsuffixed(n)
    } else {
        hex(n)
    }
}

/// Turns `n` into an unsuffixed separated hex token
pub fn hex(n: u64) -> TokenStream {
    let (h4, h3, h2, h1) = (
        (n >> 48) & 0xffff,
        (n >> 32) & 0xffff,
        (n >> 16) & 0xffff,
        n & 0xffff,
    );
    syn::parse_str::<syn::Lit>(
        &(if h4 != 0 {
            format!("0x{:04x}_{:04x}_{:04x}_{:04x}", h4, h3, h2, h1)
        } else if h3 != 0 {
            format!("0x{:04x}_{:04x}_{:04x}", h3, h2, h1)
        } else if h2 != 0 {
            format!("0x{:04x}_{:04x}", h2, h1)
        } else if h1 & 0xff00 != 0 {
            format!("0x{:04x}", h1)
        } else if h1 != 0 {
            format!("0x{:02x}", h1 & 0xff)
        } else {
            "0".to_string()
        }),
    )
    .unwrap()
    .into_token_stream()
}

/// Turns `n` into an unsuffixed token
pub fn unsuffixed(n: u64) -> TokenStream {
    Literal::u64_unsuffixed(n).into_token_stream()
}

pub fn unsuffixed_or_bool(n: u64, width: u32) -> TokenStream {
    if width == 1 {
        Ident::new(if n == 0 { "false" } else { "true" }, Span::call_site()).into_token_stream()
    } else {
        unsuffixed(n)
    }
}

pub fn register_path_to_ty(
    rpath: &svd_parser::expand::RegisterPath,
) -> Result<syn::Type, syn::Error> {
    let mut ident = format!("crate::{}", &rpath.peripheral().to_sanitized_snake_case());
    for ps in &rpath.block.path {
        ident.push_str("::");
        ident.push_str(&ps.to_sanitized_snake_case());
    }
    ident.push_str("::");
    ident.push_str(&rpath.name.to_sanitized_snake_case());
    Ok(syn::Type::Path(parse_str::<syn::TypePath>(&ident)?))
}

pub trait U32Ext {
    fn size_to_str(&self) -> Result<&str>;
    fn to_ty(&self) -> Result<Ident>;
    fn to_ty_width(&self) -> Result<u32>;
}

impl U32Ext for u32 {
    fn size_to_str(&self) -> Result<&str> {
        Ok(match *self {
            8 => "u8",
            16 => "u16",
            32 => "u32",
            64 => "u64",
            _ => {
                return Err(anyhow!(
                    "can't convert {} bits into register size type",
                    *self
                ))
            }
        })
    }
    fn to_ty(&self) -> Result<Ident> {
        Ok(Ident::new(
            match *self {
                1 => "bool",
                2..=8 => "u8",
                9..=16 => "u16",
                17..=32 => "u32",
                33..=64 => "u64",
                _ => {
                    return Err(anyhow!(
                        "can't convert {} bits into a Rust integral type",
                        *self
                    ))
                }
            },
            Span::call_site(),
        ))
    }

    fn to_ty_width(&self) -> Result<u32> {
        Ok(match *self {
            1 => 1,
            2..=8 => 8,
            9..=16 => 16,
            17..=32 => 32,
            33..=64 => 64,
            _ => {
                return Err(anyhow!(
                    "can't convert {} bits into a Rust integral type width",
                    *self
                ))
            }
        })
    }
}

/// Return the name of either register or cluster.
pub fn erc_name(erc: &RegisterCluster) -> &String {
    match erc {
        RegisterCluster::Register(r) => &r.name,
        RegisterCluster::Cluster(c) => &c.name,
    }
}

/// Return the name of either register or cluster from which this register or cluster is derived.
pub fn erc_derived_from(erc: &RegisterCluster) -> &Option<String> {
    match erc {
        RegisterCluster::Register(r) => &r.derived_from,
        RegisterCluster::Cluster(c) => &c.derived_from,
    }
}

/// Return only the clusters from the slice of either register or clusters.
pub fn only_clusters(ercs: &[RegisterCluster]) -> Vec<&Cluster> {
    ercs.iter()
        .filter_map(|x| match x {
            RegisterCluster::Cluster(x) => Some(x),
            _ => None,
        })
        .collect()
}

/// Return only the registers the given slice of either register or clusters.
pub fn only_registers(ercs: &[RegisterCluster]) -> Vec<&Register> {
    ercs.iter()
        .filter_map(|x| match x {
            RegisterCluster::Register(x) => Some(x),
            _ => None,
        })
        .collect()
}

pub fn build_rs() -> TokenStream {
    quote! {
        use std::env;
        use std::fs::File;
        use std::io::Write;
        use std::path::PathBuf;

        fn main() {
            if env::var_os("CARGO_FEATURE_RT").is_some() {
                // Put the linker script somewhere the linker can find it
                let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
                File::create(out.join("device.x"))
                    .unwrap()
                    .write_all(include_bytes!("device.x"))
                    .unwrap();
                println!("cargo:rustc-link-search={}", out.display());

                println!("cargo:rerun-if-changed=device.x");
            }

            println!("cargo:rerun-if-changed=build.rs");
        }
    }
}

pub fn handle_reg_error<T>(msg: &str, reg: &Register, res: Result<T>) -> Result<T> {
    let reg_name = &reg.name;
    let descrip = reg.description.as_deref().unwrap_or("No description");
    handle_erc_error(msg, reg_name, descrip, res)
}

pub fn handle_cluster_error<T>(msg: &str, cluster: &Cluster, res: Result<T>) -> Result<T> {
    let cluster_name = &cluster.name;
    let descrip = cluster.description.as_deref().unwrap_or("No description");
    handle_erc_error(msg, cluster_name, descrip, res)
}

fn handle_erc_error<T>(msg: &str, name: &str, descrip: &str, res: Result<T>) -> Result<T> {
    res.with_context(|| format!("{}\nName: {}\nDescription: {}", msg, name, descrip))
}

pub fn get_register_sizes(d: &Device) -> Vec<u32> {
    let mut reg_sizes = HashSet::new();
    for p in &d.peripherals {
        for r in p.all_registers() {
            if let Some(size) = r.properties.size {
                reg_sizes.insert(size);
            }
        }
    }
    let mut reg_sizes: Vec<_> = reg_sizes.into_iter().collect();
    reg_sizes.sort();
    reg_sizes
}

pub trait FullName {
    fn fullname(&self, ignore_group: bool) -> Cow<str>;
}

impl FullName for RegisterInfo {
    fn fullname(&self, ignore_group: bool) -> Cow<str> {
        match &self.alternate_group {
            Some(group) if !ignore_group => format!("{}_{}", group, self.name).into(),
            _ => self.name.as_str().into(),
        }
    }
}

impl FullName for PeripheralInfo {
    fn fullname(&self, _ignore_group: bool) -> Cow<str> {
        self.name.as_str().into()
    }
}

pub fn group_names(d: &Device) -> HashSet<Cow<str>> {
    d.peripherals
        .iter()
        .filter_map(|p| p.group_name.as_ref())
        .map(|name| name.to_sanitized_snake_case())
        .collect()
}
