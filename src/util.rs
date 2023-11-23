use std::borrow::Cow;

use crate::svd::{Access, Device, Field, RegisterInfo, RegisterProperties};
use html_escape::encode_text_minimal;
use inflections::Inflect;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std::collections::HashSet;
use svd_rs::{MaybeArray, Peripheral, PeripheralInfo};

use syn::{
    punctuated::Punctuated, token::PathSep, Lit, LitInt, PathArguments, PathSegment, Type, TypePath,
};

use anyhow::{anyhow, Result};

pub const BITS_PER_BYTE: u32 = 8;

/// List of chars that some vendors use in their peripheral/field names but
/// that are not valid in Rust ident
const BLACKLIST_CHARS: &[char] = &['(', ')', '[', ']', '/', ' ', '-'];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Case {
    Constant,
    Pascal,
    Snake,
}

impl Case {
    pub fn cow_to_case<'a>(&self, cow: Cow<'a, str>) -> Cow<'a, str> {
        match self {
            Self::Constant => match cow {
                Cow::Borrowed(s) if s.is_constant_case() => cow,
                _ => cow.to_constant_case().into(),
            },
            Self::Pascal => match cow {
                Cow::Borrowed(s) if s.is_pascal_case() => cow,
                _ => cow.to_pascal_case().into(),
            },
            Self::Snake => match cow {
                Cow::Borrowed(s) if s.is_snake_case() => cow,
                _ => cow.to_snake_case().into(),
            },
        }
    }
    pub fn sanitize<'a>(&self, s: &'a str) -> Cow<'a, str> {
        let s = if s.contains(BLACKLIST_CHARS) {
            Cow::Owned(s.replace(BLACKLIST_CHARS, ""))
        } else {
            s.into()
        };

        self.cow_to_case(s)
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
        let s = Case::Pascal.sanitize(self);
        if s.as_bytes().first().unwrap_or(&0).is_ascii_digit() {
            Cow::from(format!("_{}", s))
        } else {
            s
        }
    }
    fn to_sanitized_constant_case(&self) -> Cow<str> {
        let s = Case::Constant.sanitize(self);
        if s.as_bytes().first().unwrap_or(&0).is_ascii_digit() {
            Cow::from(format!("_{}", s))
        } else {
            s
        }
    }
    fn to_sanitized_not_keyword_snake_case(&self) -> Cow<str> {
        const INTERNALS: [&str; 4] = ["set_bit", "clear_bit", "bit", "bits"];

        let s = Case::Snake.sanitize(self);
        if s.as_bytes().first().unwrap_or(&0).is_ascii_digit() {
            format!("_{}", s).into()
        } else if INTERNALS.contains(&s.as_ref()) {
            s + "_"
        } else {
            s
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

/// Escape basic html tags and brackets
pub fn escape_special_chars(s: &str) -> String {
    let html_escaped = encode_text_minimal(s);
    escape_brackets(&html_escaped)
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
pub fn unsuffixed(n: impl Into<u64>) -> LitInt {
    LitInt::new(&n.into().to_string(), Span::call_site())
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

pub fn zst_type() -> Type {
    Type::Tuple(syn::TypeTuple {
        paren_token: syn::token::Paren::default(),
        elems: Punctuated::new(),
    })
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

pub fn type_path(segments: Punctuated<PathSegment, PathSep>) -> TypePath {
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
        //! Builder file for Peripheral access crate generated by svd2rust tool

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

pub trait FullName {
    fn fullname(&self, ignore_group: bool) -> Cow<str>;
}

impl FullName for RegisterInfo {
    fn fullname(&self, ignore_group: bool) -> Cow<str> {
        fullname(&self.name, &self.alternate_group, ignore_group)
    }
}

pub fn fullname<'a>(name: &'a str, group: &Option<String>, ignore_group: bool) -> Cow<'a, str> {
    match &group {
        Some(group) if !ignore_group => format!("{group}_{}", name).into(),
        _ => name.into(),
    }
}

impl FullName for PeripheralInfo {
    fn fullname(&self, _ignore_group: bool) -> Cow<str> {
        self.name.as_str().into()
    }
}

pub fn group_names(d: &Device) -> Vec<Cow<str>> {
    let set: HashSet<_> = d
        .peripherals
        .iter()
        .filter_map(|p| p.group_name.as_ref())
        .map(|name| name.to_sanitized_snake_case())
        .collect();
    let mut v: Vec<_> = set.into_iter().collect();
    v.sort();
    v
}

pub fn peripheral_names(d: &Device) -> Vec<String> {
    let mut v = Vec::new();
    for p in &d.peripherals {
        match p {
            Peripheral::Single(info) => {
                v.push(replace_suffix(&info.name.to_sanitized_snake_case(), ""))
            }
            Peripheral::Array(info, dim) => v.extend(
                svd_rs::array::names(info, dim).map(|n| n.to_sanitized_snake_case().into()),
            ),
        }
    }
    v.sort();
    v
}
