use std::borrow::Cow;
use std::hash::Hash;
use std::collections::{HashMap, HashSet};

use inflections::Inflect;
use svd::{Access, Cluster, Register};
use syn::Ident;
use quote::Tokens;
use either::Either;

use errors::*;

pub const BITS_PER_BYTE: u32 = 8;

/// List of chars that some vendors use in their peripheral/field names but
/// that are not valid in Rust ident
const BLACKLIST_CHARS: &'static [char] = &['(', ')', '[', ']', '/', ' '];

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

        let s = santitize_underscores(
            &self.replace(BLACKLIST_CHARS, "")
                .to_snake_case()
        );

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s))
            }
            _ => {
                keywords! {
                    s,
                    abstract,
                    alignof,
                    as,
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
        let s = santitize_underscores(
            &self.replace(BLACKLIST_CHARS, "")
                .to_upper_case()
        );

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s))
            }
            _ => Cow::from(s),
        }
    }
}

impl ToSanitizedPascalCase for str {
    fn to_sanitized_pascal_case(&self) -> Cow<str> {
        let s = santitize_underscores(
            &self.replace(BLACKLIST_CHARS, "")
                .to_pascal_case()
        );

        match s.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", s))
            }
            _ => Cow::from(s),
        }
    }
}

pub fn respace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn santitize_underscores(s: &str) -> String {
    s.split('_')
        .filter(|part| part.len() > 0)
        .collect::<Vec<_>>()
        .join("_")
}

pub fn name_of(register: &Register) -> Cow<str> {
    match *register {
        Register::Single(ref info) => Cow::from(&*info.name),
        Register::Array(ref info, _) => if info.name.contains("[%s]") {
            info.name.replace("[%s]", "").into()
        } else {
            info.name.replace("%s", "").into()
        },
    }
}

pub fn set_name_of(register: &mut Register, name: String) {
    match *register {
        Register::Single(ref mut info) => info.name = name,
        Register::Array(ref mut info, _) => info.name = name,
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
            _ => Err(format!(
                "can't convert {} bits into a Rust integral type width",
                *self
            ))?,
        })
    }
}

/// Return only the clusters from the slice of either register or clusters.
pub fn only_clusters(ercs: &[Either<Register, Cluster>]) -> Vec<&Cluster> {
    let clusters: Vec<&Cluster> = ercs.iter()
        .filter_map(|x| match *x {
            Either::Right(ref x) => Some(x),
            _ => None,
        })
        .collect();
    clusters
}

/// Return only the registers the given slice of either register or clusters.
pub fn only_registers(ercs: &[Either<Register, Cluster>]) -> Vec<&Register> {
    let registers: Vec<&Register> = ercs.iter()
        .filter_map(|x| match *x {
            Either::Left(ref x) => Some(x),
            _ => None,
        })
        .collect();
    registers
}

/// Renames registers if their name occurs multiple times
pub fn registers_with_uniq_names<'a, I: Iterator<Item = &'a Register>>(registers: I) -> Vec<Cow<'a, Register>> {
    let (capacity, _) = registers.size_hint();
    let mut seen = HashSet::with_capacity(capacity);
    registers.map(|register| {
        let mut n = 1;
        let mut name = name_of(&*register);
        let mut dup = false;
        // Count up `n` until register name is not already present
        // in `seen`
        while seen.contains(&name) {
            dup = true;
            n += 1;
            name = Cow::Owned(format!("{}_{}", name_of(&*register), n));
        }
        seen.insert(name.clone());

        if dup {
            let mut register = register.clone();
            set_name_of(&mut register, name.into_owned());
            Cow::Owned(register)
        } else {
            Cow::Borrowed(register)
        }
    }).collect()
}

fn count_occurrences<'a, K, I>(iter: I) -> HashMap<K, usize>
where
    K: Eq + Hash,
    I: Iterator<Item = K>,
{
    let mut counts = HashMap::new();
    for k in iter {
        let count = counts.entry(k)
            .or_insert(0);
        *count += 1;
    }
    counts
}

// Generically rename identifiers that occur multiple times into a
// series where both `sc` and `pc` end in `…_1`, `…_2`, and so on.
pub fn rename_identifiers<E, K, G, S>(entries: &mut Vec<E>, getter: G, setter: S)
where
    K: Eq + Hash + Clone,
    G: Fn(&E) -> K,
    S: Fn(&mut E, usize),
{
    let counts = count_occurrences(
        entries.iter()
            .map(|entry| getter(entry))
    );
    // Rename identifiers that occur multiple times into a
    // series where both `sc` and `pc` end in `…_1`,
    // `…_2`, and so on.
    let mut indexes = HashMap::<K, usize>::new();
    for entry in entries.iter_mut() {
        let key = getter(entry);
        match counts.get(&key) {
            Some(count) if *count > 1 => {
                let index = indexes.entry(key).or_insert(0);
                *index += 1;

                setter(entry, *index);
            }
            _ => {}
        }
    }
}
