use std::borrow::Cow;

use crate::svd::{Access, Device, DimElement, Field, RegisterInfo, RegisterProperties};
use inflections::Inflect;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use svd_parser::expand::BlockPath;
use svd_rs::{MaybeArray, PeripheralInfo};

use syn::{
    punctuated::Punctuated, token::Colon2, AngleBracketedGenericArguments, GenericArgument, Lit,
    LitInt, PathArguments, PathSegment, Token, Type, TypePath,
};

use anyhow::{anyhow, bail, Result};

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
    fn to_pascal_case_ident(&self, span: Span) -> Ident {
        Ident::new(&self.to_sanitized_pascal_case(), span)
    }
    /// Convert self into CONSTANT_CASE.
    ///
    /// Use on name of reader structs, writer structs and enumerations.
    fn to_sanitized_constant_case(&self) -> Cow<str>;
    fn to_constant_case_ident(&self, span: Span) -> Ident {
        Ident::new(&self.to_sanitized_constant_case(), span)
    }
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
    fn to_snake_case_ident(&self, span: Span) -> Ident {
        Ident::new(&self.to_sanitized_snake_case(), span)
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

pub fn digit_or_hex(n: u64) -> LitInt {
    if n < 10 {
        unsuffixed(n)
    } else {
        hex(n)
    }
}

/// Turns `n` into an unsuffixed separated hex token
pub fn hex(n: u64) -> LitInt {
    let (h4, h3, h2, h1) = (
        (n >> 48) & 0xffff,
        (n >> 32) & 0xffff,
        (n >> 16) & 0xffff,
        n & 0xffff,
    );
    LitInt::new(
        &(if h4 != 0 {
            format!("0x{h4:04x}_{h3:04x}_{h2:04x}_{h1:04x}")
        } else if h3 != 0 {
            format!("0x{h3:04x}_{h2:04x}_{h1:04x}")
        } else if h2 != 0 {
            format!("0x{h2:04x}_{h1:04x}")
        } else if h1 & 0xff00 != 0 {
            format!("0x{h1:04x}")
        } else if h1 != 0 {
            format!("0x{:02x}", h1 & 0xff)
        } else {
            "0".to_string()
        }),
        Span::call_site(),
    )
}

/// Turns `n` into an unsuffixed token
pub fn unsuffixed(n: u64) -> LitInt {
    LitInt::new(&n.to_string(), Span::call_site())
}

pub fn unsuffixed_or_bool(n: u64, width: u32) -> Lit {
    if width == 1 {
        Lit::Bool(syn::LitBool::new(n != 0, Span::call_site()))
    } else {
        Lit::Int(unsuffixed(n))
    }
}

pub fn new_syn_u32(len: u32, span: Span) -> syn::Expr {
    syn::Expr::Lit(syn::ExprLit {
        attrs: Vec::new(),
        lit: syn::Lit::Int(syn::LitInt::new(&len.to_string(), span)),
    })
}

pub fn array_proxy_type(ty: Type, array_info: &DimElement) -> Type {
    let span = Span::call_site();
    let inner_path = GenericArgument::Type(ty);
    let mut args = Punctuated::new();
    args.push(inner_path);
    args.push(GenericArgument::Const(new_syn_u32(array_info.dim, span)));
    args.push(GenericArgument::Const(syn::Expr::Lit(syn::ExprLit {
        attrs: Vec::new(),
        lit: syn::Lit::Int(hex(array_info.dim_increment as u64)),
    })));
    let arguments = PathArguments::AngleBracketed(AngleBracketedGenericArguments {
        colon2_token: None,
        lt_token: Token![<](span),
        args,
        gt_token: Token![>](span),
    });

    let mut segments = Punctuated::new();
    segments.push(path_segment(Ident::new("crate", span)));
    segments.push(PathSegment {
        ident: Ident::new("ArrayProxy", span),
        arguments,
    });
    Type::Path(type_path(segments))
}

pub fn name_to_ty(name: &str) -> Type {
    let span = Span::call_site();
    let mut segments = Punctuated::new();
    segments.push(path_segment(name.to_constant_case_ident(span)));
    syn::Type::Path(type_path(segments))
}

pub fn block_path_to_ty(bpath: &svd_parser::expand::BlockPath, span: Span) -> TypePath {
    let mut segments = Punctuated::new();
    segments.push(path_segment(Ident::new("crate", span)));
    segments.push(path_segment(bpath.peripheral.to_snake_case_ident(span)));
    for ps in &bpath.path {
        segments.push(path_segment(ps.to_snake_case_ident(span)));
    }
    type_path(segments)
}

pub fn register_path_to_ty(rpath: &svd_parser::expand::RegisterPath, span: Span) -> TypePath {
    let mut p = block_path_to_ty(&rpath.block, span);
    p.path
        .segments
        .push(path_segment(rpath.name.to_snake_case_ident(span)));
    p
}

pub fn ident_to_path(ident: Ident) -> TypePath {
    let mut segments = Punctuated::new();
    segments.push(path_segment(ident));
    type_path(segments)
}

pub fn type_path(segments: Punctuated<PathSegment, Colon2>) -> TypePath {
    TypePath {
        qself: None,
        path: syn::Path {
            leading_colon: None,
            segments,
        },
    }
}

pub fn path_segment(ident: Ident) -> PathSegment {
    PathSegment {
        ident,
        arguments: PathArguments::None,
    }
}

pub fn parent(p: &BlockPath) -> BlockPath {
    let mut p = p.clone();
    p.path.pop().unwrap();
    p
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
            Some(group) if !ignore_group => format!("{group}_{}", self.name).into(),
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
