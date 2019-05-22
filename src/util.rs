use std::borrow::Cow;

use either::Either;
use inflections::Inflect;
use quote::Tokens;
use svd::{Access, Cluster, Register};
use syn::Ident;

use errors::*;

pub const BITS_PER_BYTE: u32 = 8;

/// List of chars that some vendors use in their peripheral/field names but
/// that are not valid in Rust ident
const BLACKLIST_CHARS: &[char] = &['(', ')', '[', ']', '/', ' '];

#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    CortexM,
    Msp430,
    RISCV,
    None,
}

impl Target {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "cortex-m" => Target::CortexM,
            "msp430" => Target::Msp430,
            "riscv" => Target::RISCV,
            "none" => Target::None,
            _ => bail!("unknown target {}", s),
        })
    }
}

pub trait ToSanitizedPascalCase {
    fn to_sanitized_pascal_case(&self) -> Cow<str>;
}

pub trait ToSanitizedUpperCase {
    fn to_sanitized_upper_case(&self) -> Cow<str>;
}

pub trait ToSanitizedSnakeCase {
    fn to_sanitized_snake_case(&self) -> Cow<str>;
}

impl ToSanitizedSnakeCase for str {
    fn to_sanitized_snake_case(&self) -> Cow<str> {
        macro_rules! keywords {
            ($s:expr, $($kw:ident),+,) => {
                Cow::from(match &$s.to_lowercase()[..] {
                    $(stringify!($kw) => concat!(stringify!($kw), "_")),+,
                    _ => return Cow::from($s.to_snake_case())
                })
            }
        }

        let s = self.replace(BLACKLIST_CHARS, "");

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s.to_snake_case()))
            }
            _ => {
                keywords! {
                    s,
                    abstract,
                    alignof,
                    as,
                    async,
                    await,
                    become,
                    box,
                    break,
                    const,
                    continue,
                    crate,
                    do,
                    else,
                    enum,
                    extern,
                    false,
                    final,
                    fn,
                    for,
                    if,
                    impl,
                    in,
                    let,
                    loop,
                    macro,
                    match,
                    mod,
                    move,
                    mut,
                    offsetof,
                    override,
                    priv,
                    proc,
                    pub,
                    pure,
                    ref,
                    return,
                    self,
                    sizeof,
                    static,
                    struct,
                    super,
                    trait,
                    true,
                    try,
                    type,
                    typeof,
                    unsafe,
                    unsized,
                    use,
                    virtual,
                    where,
                    while,
                    yield,
                    set_bit,
                    clear_bit,
                    bit,
                    bits,
                }
            }
        }
    }
}

impl ToSanitizedUpperCase for str {
    fn to_sanitized_upper_case(&self) -> Cow<str> {
        let s = self.replace(BLACKLIST_CHARS, "");

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s.to_upper_case()))
            }
            _ => Cow::from(s.to_upper_case()),
        }
    }
}

impl ToSanitizedPascalCase for str {
    fn to_sanitized_pascal_case(&self) -> Cow<str> {
        let s = self.replace(BLACKLIST_CHARS, "");

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s.to_pascal_case()))
            }
            _ => Cow::from(s.to_pascal_case()),
        }
    }
}

pub fn respace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn escape_brackets(s: &str) -> String {
    s.split('[')
        .fold("".to_string(), |acc, x| {
            if acc == "" {
                x.to_string()
            } else if acc.ends_with('\\') {
                acc.to_owned() + "[" + &x.to_string()
            } else {
                acc.to_owned() + "\\[" + &x.to_string()
            }
        })
        .split(']')
        .fold("".to_string(), |acc, x| {
            if acc == "" {
                x.to_string()
            } else if acc.ends_with('\\') {
                acc.to_owned() + "]" + &x.to_string()
            } else {
                acc.to_owned() + "\\]" + &x.to_string()
            }
        })
}

pub fn name_of(register: &Register) -> Cow<str> {
    match *register {
        Register::Single(ref info) => Cow::from(&*info.name),
        Register::Array(ref info, _) => {
            if info.name.contains("[%s]") {
                info.name.replace("[%s]", "").into()
            } else {
                info.name.replace("%s", "").into()
            }
        }
    }
}

pub fn access_of(register: &Register) -> Access {
    register.access.unwrap_or_else(|| {
        if let Some(ref fields) = register.fields {
            if fields.iter().all(|f| f.access == Some(Access::ReadOnly)) {
                Access::ReadOnly
            } else if fields.iter().all(|f| f.access == Some(Access::WriteOnly)) {
                Access::WriteOnly
            } else {
                Access::ReadWrite
            }
        } else {
            Access::ReadWrite
        }
    })
}

/// Turns `n` into an unsuffixed separated hex token
pub fn hex(n: u32) -> Tokens {
    let mut t = Tokens::new();
    let (h2, h1) = ((n >> 16) & 0xffff, n & 0xffff);
    t.append(if h2 != 0 {
        format!("0x{:04x}_{:04x}", h2, h1)
    } else if h1 & 0xff00 != 0 {
        format!("0x{:04x}", h1)
    } else if h1 != 0 {
        format!("0x{:02x}", h1 & 0xff)
    } else {
        String::from("0")
    });
    t
}

pub fn hex_or_bool(n: u32, width: u32) -> Tokens {
    if width == 1 {
        let mut t = Tokens::new();
        t.append(if n == 0 { "false" } else { "true" });
        t
    } else {
        hex(n)
    }
}

/// Turns `n` into an unsuffixed token
pub fn unsuffixed(n: u64) -> Tokens {
    let mut t = Tokens::new();
    t.append(format!("{}", n));
    t
}

pub fn unsuffixed_or_bool(n: u64, width: u32) -> Tokens {
    if width == 1 {
        let mut t = Tokens::new();
        t.append(if n == 0 { "false" } else { "true" });
        t
    } else {
        unsuffixed(n)
    }
}

pub trait U32Ext {
    fn to_ty(&self) -> Result<Ident>;
    fn to_ty_width(&self) -> Result<u32>;
}

impl U32Ext for u32 {
    fn to_ty(&self) -> Result<Ident> {
        Ok(match *self {
            1 => Ident::new("bool"),
            2...8 => Ident::new("u8"),
            9...16 => Ident::new("u16"),
            17...32 => Ident::new("u32"),
            33...64 => Ident::new("u64"),
            _ => Err(format!(
                "can't convert {} bits into a Rust integral type",
                *self
            ))?,
        })
    }

    fn to_ty_width(&self) -> Result<u32> {
        Ok(match *self {
            1 => 1,
            2...8 => 8,
            9...16 => 16,
            17...32 => 32,
            33...64 => 64,
            _ => Err(format!(
                "can't convert {} bits into a Rust integral type width",
                *self
            ))?,
        })
    }
}

/// Return only the clusters from the slice of either register or clusters.
pub fn only_clusters(ercs: &[Either<Register, Cluster>]) -> Vec<&Cluster> {
    let clusters: Vec<&Cluster> = ercs
        .iter()
        .filter_map(|x| match *x {
            Either::Right(ref x) => Some(x),
            _ => None,
        })
        .collect();
    clusters
}

/// Return only the registers the given slice of either register or clusters.
pub fn only_registers(ercs: &[Either<Register, Cluster>]) -> Vec<&Register> {
    let registers: Vec<&Register> = ercs
        .iter()
        .filter_map(|x| match *x {
            Either::Left(ref x) => Some(x),
            _ => None,
        })
        .collect();
    registers
}

pub fn build_rs() -> Tokens {
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
