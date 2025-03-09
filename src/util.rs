use std::borrow::Cow;

pub use crate::config::{Case, IdentFormat};
use crate::{
    svd::{Access, Device, Field, RegisterInfo, RegisterProperties},
    Config,
};
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

fn to_pascal_case(s: &str) -> String {
    if !s.contains('_') {
        s.to_pascal_case()
    } else {
        let mut string = String::new();
        let mut parts = s.split('_').peekable();
        if let Some(&"") = parts.peek() {
            string.push('_');
        }
        while let Some(p) = parts.next() {
            if p.is_empty() {
                continue;
            }
            string.push_str(&p.to_pascal_case());
            match parts.peek() {
                Some(nxt)
                    if p.ends_with(|s: char| s.is_numeric())
                        && nxt.starts_with(|s: char| s.is_numeric()) =>
                {
                    string.push('_');
                }
                Some(&"") => {
                    string.push('_');
                }
                _ => {}
            }
        }
        string
    }
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
                _ => to_pascal_case(&cow).into(),
            },
            Self::Snake => match cow {
                Cow::Borrowed(s) if s.is_snake_case() => cow,
                _ => cow.to_snake_case().into(),
            },
        }
    }

    pub fn sanitize<'a>(&self, s: &'a str) -> Cow<'a, str> {
        let s = sanitize(s);
        self.cow_to_case(s)
    }
}

fn sanitize(s: &str) -> Cow<'_, str> {
    if s.contains(BLACKLIST_CHARS) {
        Cow::Owned(s.replace(BLACKLIST_CHARS, ""))
    } else {
        s.into()
    }
}

pub fn ident(name: &str, config: &Config, fmt: &str, span: Span) -> Ident {
    Ident::new(
        &config
            .ident_formats
            .get(fmt)
            .expect("Missing {fmt} entry")
            .sanitize(name),
        span,
    )
}

impl IdentFormat {
    pub fn apply<'a>(&self, name: &'a str) -> Cow<'a, str> {
        let name = match &self.case {
            Some(case) => case.sanitize(name),
            _ => sanitize(name),
        };
        if self.prefix.is_empty() && self.suffix.is_empty() {
            name
        } else {
            format!("{}{}{}", self.prefix, name, self.suffix).into()
        }
    }
    pub fn sanitize<'a>(&self, name: &'a str) -> Cow<'a, str> {
        let s = self.apply(name);
        let s = if s.as_bytes().first().unwrap_or(&0).is_ascii_digit() {
            Cow::from(format!("_{}", s))
        } else {
            s
        };
        match self.case {
            Some(Case::Snake) | None => sanitize_keyword(s),
            _ => s,
        }
    }
}

pub fn ident_str(name: &str, fmt: &IdentFormat) -> String {
    let name = match &fmt.case {
        Some(case) => case.sanitize(name),
        _ => sanitize(name),
    };
    format!("{}{}{}", fmt.prefix, name, fmt.suffix)
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

pub fn respace(s: &str) -> Cow<'_, str> {
    if s.contains("\n\n ") {
        let ss: Vec<_> = s.split("\n\n").map(|s| s.trim_start()).collect();
        ss.join("\n\n").into()
    } else {
        s.into()
    }
}

pub fn escape_brackets(s: &str) -> String {
    s.split('[')
        .fold(String::new(), |acc, x| {
            if acc.is_empty() {
                x.to_string()
            } else if acc.ends_with('\\') {
                acc + "[" + x
            } else {
                acc + "\\[" + x
            }
        })
        .split(']')
        .fold(String::new(), |acc, x| {
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
pub fn escape_special_chars(s: &str) -> Cow<'_, str> {
    if s.contains('[') {
        escape_brackets(s).into()
    } else {
        s.into()
    }
}

pub fn name_of<T: FullName>(maybe_array: &MaybeArray<T>, ignore_group: bool) -> String {
    let fullname = maybe_array.fullname(ignore_group);
    if maybe_array.is_array() {
        fullname.remove_dim().into()
    } else {
        fullname.into()
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

/// Turns non-zero `n` into an unsuffixed separated hex token
pub fn hex_nonzero(n: u64) -> Option<LitInt> {
    (n != 0).then(|| hex(n))
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

pub fn name_to_ty(name: Ident) -> Type {
    let mut segments = Punctuated::new();
    segments.push(path_segment(name));
    syn::Type::Path(type_path(segments))
}

pub fn block_path_to_ty(
    bpath: &svd_parser::expand::BlockPath,
    config: &Config,
    span: Span,
) -> TypePath {
    let mut path = config.settings.crate_path.clone().unwrap_or_default().0;
    path.segments.push(path_segment(ident(
        &bpath.peripheral.remove_dim(),
        config,
        "peripheral_mod",
        span,
    )));
    for ps in &bpath.path {
        path.segments.push(path_segment(ident(
            &ps.remove_dim(),
            config,
            "cluster_mod",
            span,
        )));
    }
    TypePath { qself: None, path }
}

pub fn register_path_to_ty(
    rpath: &svd_parser::expand::RegisterPath,
    config: &Config,
    span: Span,
) -> TypePath {
    let mut p = block_path_to_ty(&rpath.block, config, span);
    p.path.segments.push(path_segment(ident(
        &rpath.name.remove_dim(),
        config,
        "register_mod",
        span,
    )));
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
            _ => return Err(anyhow!("can't convert {self} bits into register size type")),
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
                        "can't convert {self} bits into a Rust integral type"
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
                    "can't convert {self} bits into a Rust integral type width"
                ))
            }
        })
    }
}

pub fn build_rs(config: &Config) -> TokenStream {
    let extra_build = config.extra_build();

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

                #extra_build
            }

            println!("cargo:rerun-if-changed=build.rs");
        }
    }
}

pub trait DimSuffix {
    fn expand_dim(&self, suffix: &str) -> Cow<str>;
    fn remove_dim(&self) -> Cow<str> {
        self.expand_dim("")
    }
}

impl DimSuffix for str {
    fn expand_dim(&self, suffix: &str) -> Cow<str> {
        if self.contains("%s") {
            self.replace(if self.contains("[%s]") { "[%s]" } else { "%s" }, suffix)
                .into()
        } else {
            self.into()
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

pub fn group_names<'a>(d: &'a Device, feature_format: &'a IdentFormat) -> Vec<Cow<'a, str>> {
    let set: HashSet<_> = d
        .peripherals
        .iter()
        .filter_map(|p| p.group_name.as_ref())
        .map(|name| feature_format.apply(name))
        .collect();
    let mut v: Vec<_> = set.into_iter().collect();
    v.sort();
    v
}

pub fn peripheral_names(d: &Device, feature_format: &IdentFormat) -> Vec<String> {
    let mut v = Vec::new();
    for p in &d.peripherals {
        match p {
            Peripheral::Single(info) => {
                v.push(feature_format.apply(&info.name).remove_dim().into());
            }
            Peripheral::Array(info, dim) => {
                v.extend(svd_rs::array::names(info, dim).map(|n| feature_format.apply(&n).into()));
            }
        }
    }
    v.sort();
    v
}

#[test]
fn pascalcase() {
    assert_eq!(to_pascal_case("_reserved"), "_Reserved");
    assert_eq!(to_pascal_case("_FOO_BAR_"), "_FooBar_");
    assert_eq!(to_pascal_case("FOO_BAR1"), "FooBar1");
    assert_eq!(to_pascal_case("FOO_BAR_1"), "FooBar1");
    assert_eq!(to_pascal_case("FOO_BAR_1_2"), "FooBar1_2");
    assert_eq!(to_pascal_case("FOO_BAR_1_2_"), "FooBar1_2_");
}
