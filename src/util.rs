use std::borrow::Cow;
use std::rc::Rc;

use either::Either;
use inflections::Inflect;
use svd::{Access, EnumeratedValues, Field, Peripheral, Register, RegisterInfo,
          Usage};
use syn::{Ident, IntTy, Lit};

use errors::*;

/// List of chars that some vendors use in their peripheral/field names but
/// that are not valid in Rust ident
const BLACKLIST_CHARS: &'static [char] = &['(', ')'];

pub trait ToSanitizedPascalCase {
    fn to_sanitized_pascal_case(&self) -> Cow<str>;
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
                }
            }
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

pub struct ExpandedRegister<'a> {
    pub info: &'a RegisterInfo,
    pub name: String,
    pub offset: u32,
    pub ty: Either<String, Rc<String>>,
}

/// Takes a list of "registers", some of which may actually be register arrays,
/// and turns it into a new *sorted* (by address offset) list of registers where
/// the register arrays have been expanded.
pub fn expand(registers: &[Register]) -> Vec<ExpandedRegister> {
    let mut out = vec![];

    for r in registers {
        match *r {
            Register::Single(ref info) => {
                out.push(
                    ExpandedRegister {
                        info: info,
                        name: info.name.to_sanitized_snake_case().into_owned(),
                        offset: info.address_offset,
                        ty: Either::Left(
                            info.name
                                .to_sanitized_pascal_case()
                                .into_owned(),
                        ),
                    },
                )
            }
            Register::Array(ref info, ref array_info) => {
                let has_brackets = info.name.contains("[%s]");

                let ty = if has_brackets {
                    info.name.replace("[%s]", "")
                } else {
                    info.name.replace("%s", "")
                };

                let ty = Rc::new(ty.to_sanitized_pascal_case().into_owned());

                let indices = array_info
                    .dim_index
                    .as_ref()
                    .map(|v| Cow::from(&**v))
                    .unwrap_or_else(
                        || {
                            Cow::from(
                                (0..array_info.dim)
                                    .map(|i| i.to_string())
                                    .collect::<Vec<_>>(),
                            )
                        },
                    );

                for (idx, i) in indices.iter().zip(0..) {
                    let name = if has_brackets {
                        info.name.replace("[%s]", idx)
                    } else {
                        info.name.replace("%s", idx)
                    };

                    let offset = info.address_offset +
                                 i * array_info.dim_increment;

                    out.push(
                        ExpandedRegister {
                            info: info,
                            name: name.to_sanitized_snake_case().into_owned(),
                            offset: offset,
                            ty: Either::Right(ty.clone()),
                        },
                    );
                }
            }
        }
    }

    out.sort_by_key(|x| x.offset);

    out
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
    register
        .access
        .unwrap_or_else(
            || if let Some(ref fields) = register.fields {
                if fields.iter().all(|f| f.access == Some(Access::ReadOnly)) {
                    Access::ReadOnly
                } else if fields
                              .iter()
                              .all(|f| f.access == Some(Access::WriteOnly),) {
                    Access::WriteOnly
                } else {
                    Access::ReadWrite
                }
            } else {
                Access::ReadWrite
            },
        )
}

/// Turns `n` into an unsuffixed literal
pub fn unsuffixed(n: u64) -> Lit {
    Lit::Int(n, IntTy::Unsuffixed)
}

pub fn unsuffixed_or_bool(n: u64, width: u32) -> Lit {
    if width == 1 {
        if n == 0 {
            Lit::Bool(false)
        } else {
            Lit::Bool(true)
        }
    } else {
        unsuffixed(n)
    }
}

#[derive(Clone, Debug)]
pub struct Base<'a> {
    pub register: Option<&'a str>,
    pub field: &'a str,
}

pub fn lookup<'a>
    (
    evs: &'a [EnumeratedValues],
    fields: &'a [Field],
    register: &'a Register,
    all_registers: &'a [Register],
    peripheral: &'a Peripheral,
    all_peripherals: &'a [Peripheral],
    usage: Usage,
) -> Result<Option<(&'a EnumeratedValues, Option<Base<'a>>)>> {
    let evs = evs.iter()
        .map(
            |evs| if let Some(ref base) = evs.derived_from {
                let mut parts = base.split('.');

                match (parts.next(), parts.next(), parts.next(), parts.next()) {
                    (Some(base_peripheral), Some(base_register), Some(base_field), Some(base_evs)) => {
                        lookup_in_peripherals(
                            base_peripheral,
                            base_register,
                            base_field,
                            base_evs,
                            all_peripherals,
                        )
                    }
                    (Some(base_register), Some(base_field), Some(base_evs), None) => {
                        lookup_in_peripheral(
                            base_register,
                            base_field,
                            base_evs,
                            all_registers,
                            peripheral,
                        )
                    }
                    (Some(base_field), Some(base_evs), None, None) => {
                        lookup_in_fields(base_evs, base_field, fields, register)
                    }
                    (Some(base_evs), None, None, None) => {
                        lookup_in_register(base_evs, register)
                    }
                    _ => unreachable!(),
                }
            } else {
                Ok((evs, None))
            },
        )
        .collect::<Result<Vec<_>>>()?;

    for &(ref evs, ref base) in evs.iter() {
        if evs.usage == Some(usage) {
            return Ok(Some((*evs, base.clone())));
        }
    }

    Ok(evs.first().cloned())
}

fn lookup_in_fields<'f>(
    base_evs: &str,
    base_field: &str,
    fields: &'f [Field],
    register: &Register,
) -> Result<(&'f EnumeratedValues, Option<Base<'f>>)> {
    if let Some(base_field) = fields.iter().find(|f| f.name == base_field) {
        return lookup_in_field(base_evs, None, base_field);
    } else {
        Err(
            format!(
                "Field {} not found in register {}",
                base_field,
                register.name
            ),
        )?
    }
}

fn lookup_in_peripheral<'p>
    (
    base_register: &'p str,
    base_field: &str,
    base_evs: &str,
    all_registers: &'p [Register],
    peripheral: &'p Peripheral,
) -> Result<(&'p EnumeratedValues, Option<Base<'p>>)> {
    if let Some(register) = all_registers.iter().find(
        |r| {
            r.name == base_register
        },
    ) {
        if let Some(field) = register
               .fields
               .as_ref()
               .map(|fs| &**fs)
               .unwrap_or(&[])
               .iter()
               .find(|f| f.name == base_field) {
            lookup_in_field(base_evs, Some(base_register), field)
        } else {
            Err(
                format!(
                    "No field {} in register {}",
                    base_field,
                    register.name
                ),
            )?
        }
    } else {
        Err(
            format!(
                "No register {} in peripheral {}",
                base_register,
                peripheral.name
            ),
        )?
    }
}

fn lookup_in_field<'f>(
    base_evs: &str,
    base_register: Option<&'f str>,
    field: &'f Field,
) -> Result<(&'f EnumeratedValues, Option<Base<'f>>)> {
    for evs in &field.enumerated_values {
        if evs.name.as_ref().map(|s| &**s) == Some(base_evs) {
            return Ok(
                ((evs,
                  Some(
                    Base {
                        field: &field.name,
                        register: base_register,
                    },
                ))),
            );
        }
    }

    Err(format!("No EnumeratedValues {} in field {}", base_evs, field.name),)?
}

fn lookup_in_register<'r>
    (
    base_evs: &str,
    register: &'r Register,
) -> Result<(&'r EnumeratedValues, Option<Base<'r>>)> {
    let mut matches = vec![];

    for f in register.fields.as_ref().map(|v| &**v).unwrap_or(&[]) {
        if let Some(evs) =
            f.enumerated_values
                .iter()
                .find(|evs| evs.name.as_ref().map(|s| &**s) == Some(base_evs)) {
            matches.push((evs, &f.name))
        }
    }

    match matches.first() {
        None => {
            Err(
                format!(
                    "EnumeratedValues {} not found in register {}",
                    base_evs,
                    register.name
                ),
            )?
        }
        Some(&(evs, field)) => {
            if matches.len() == 1 {
                return Ok(
                    (evs,
                     Some(
                        Base {
                            field: field,
                            register: None,
                        },
                    )),
                );
            } else {
                let fields = matches
                    .iter()
                    .map(|&(ref f, _)| &f.name)
                    .collect::<Vec<_>>();
                Err(
                    format!(
                        "Fields {:?} have an \
                                             enumeratedValues named {}",
                        fields,
                        base_evs
                    ),
                )?
            }
        }
    }
}

fn lookup_in_peripherals<'p>
(
    base_peripheral: &'p str,
    base_register: &'p str,
    base_field: &str,
    base_evs: &str,
    all_peripherals: &'p [Peripheral],
) -> Result<(&'p EnumeratedValues, Option<Base<'p>>)> {
    if let Some(peripheral) = all_peripherals
        .iter()
        .find(|p| { p.name == base_peripheral }) {
        let all_registers = peripheral.registers
            .as_ref()
            .map(|x| x.as_ref())
            .unwrap_or(&[][..]);
        lookup_in_peripheral(base_register, base_field, base_evs, all_registers, peripheral)
    } else {
        Err(
            format!(
                "No peripheral {}",
                base_peripheral
            ),
        )?
    }
}


pub trait U32Ext {
    fn to_ty(&self) -> Result<Ident>;
    fn to_ty_width(&self) -> Result<u32>;
}

impl U32Ext for u32 {
    fn to_ty(&self) -> Result<Ident> {
        Ok(
            match *self {
                1 => Ident::new("bool"),
                2...8 => Ident::new("u8"),
                9...16 => Ident::new("u16"),
                17...32 => Ident::new("u32"),
                _ => {
                    Err(
                        format!(
                            "can't convert {} bits into a Rust integral type",
                            *self
                        ),
                    )?
                }
            },
        )
    }

    fn to_ty_width(&self) -> Result<u32> {
        Ok(
            match *self {
                1 => 1,
                2...8 => 8,
                9...16 => 16,
                17...32 => 32,
                _ => {
                    Err(
                        format!(
                            "can't convert {} bits into a Rust integral type width",
                            *self
                        ),
                    )?
                }
            }
        )
    }
}
