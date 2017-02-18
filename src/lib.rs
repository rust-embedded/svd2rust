//! Generate Rust register maps (`struct`s) from SVD files
//!
//! # [Changelog](https://github.com/japaric/svd2rust/blob/master/CHANGELOG.md)
//!
//! # Installation
//!
//! ```
//! $ cargo install svd2rust
//! ```
//!
//! # Usage
//!
//! - Get the start/base address of each peripheral's register block.
//!
//! ```
//! $ svd2rust -i STM32F30x.svd
//! const GPIOA: usize = 0x48000000;
//! const GPIOB: usize = 0x48000400;
//! const GPIOC: usize = 0x48000800;
//! const GPIOD: usize = 0x48000c00;
//! const GPIOE: usize = 0x48001000;
//! const GPIOF: usize = 0x48001400;
//! const TSC: usize = 0x40024000;
//! const CRC: usize = 0x40023000;
//! const Flash: usize = 0x40022000;
//! const RCC: usize = 0x40021000;
//! ```
//!
//! - Generate a register map for a single peripheral.
//!
//! ```
//! $ svd2rust -i STM32F30x.svd rcc | head
//! /// Reset and clock control
//! #[repr(C)]
//! pub struct Rcc {
//!     /// 0x00 - Clock control register
//!     pub cr: Cr,
//!     /// 0x04 - Clock configuration register (RCC_CFGR)
//!     pub cfgr: Cfgr,
//!     /// 0x08 - Clock interrupt register (RCC_CIR)
//!     pub cir: Cir,
//!     /// 0x0c - APB2 peripheral reset register (RCC_APB2RSTR)
//! ```
//!
//! # API
//!
//! The `svd2rust` generates the following API for each peripheral:
//!
//! ## Register block
//!
//! A register block "definition" as a `struct`. Example below:
//!
//! ``` rust
//! /// Inter-integrated circuit
//! #[repr(C)]
//! pub struct I2c1 {
//!     /// 0x00 - Control register 1
//!     pub cr1: Cr1,
//!     /// 0x04 - Control register 2
//!     pub cr2: Cr2,
//!     /// 0x08 - Own address register 1
//!     pub oar1: Oar1,
//!     /// 0x0c - Own address register 2
//!     pub oar2: Oar2,
//!     /// 0x10 - Timing register
//!     pub timingr: Timingr,
//!     /// 0x14 - Status register 1
//!     pub timeoutr: Timeoutr,
//!     /// 0x18 - Interrupt and Status register
//!     pub isr: Isr,
//!     /// 0x1c - Interrupt clear register
//!     pub icr: Icr,
//!     /// 0x20 - PEC register
//!     pub pecr: Pecr,
//!     /// 0x24 - Receive data register
//!     pub rxdr: Rxdr,
//!     /// 0x28 - Transmit data register
//!     pub txdr: Txdr,
//! }
//! ```
//!
//! The user has to "instantiate" this definition for each peripheral the
//! microcontroller has. They have two choices:
//!
//! - `static` variables. Example below:
//!
//! ``` rust
//! extern "C" {
//!     // I2C1 can be accessed in read-write mode
//!     pub static mut I2C1: I2c;
//!     // whereas I2C2 can only be accessed in "read-only" mode
//!     pub static I2C1: I2c;
//! }
//! ```
//!
//! here the addresses of these register blocks must be provided by a linker
//! script:
//!
//! ``` ld
//! /* layout.ld */
//! I2C1 = 0x40005400;
//! I2C2 = 0x40005800;
//! ```
//!
//! This has the side effect that the `I2C1` and `I2C2` symbols get "taken" so
//! no other C/Rust symbol (`static`, `function`, etc.) can have the same name.
//!
//! - "constructor" functions. Example below:
//!
//! ``` rust
//! // Base addresses of the register blocks. These are private.
//! const I2C1: usize = 0x40005400;
//! const I2C2: usize = 0x40005800;
//!
//! // NOTE(unsafe) hands out aliased `&mut-` references
//! pub unsafe fn i2c1() -> &'static mut I2C {
//!     unsafe { &mut *(I2C1 as *mut I2c) }
//! }
//!
//! pub fn i2c2() -> &'static I2C {
//!     unsafe { &*(I2C2 as *const I2c) }
//! }
//! ```
//!
//! ## `read` / `modify` / `write`
//!
//! Each register in the register block, e.g. the `cr1` field in the `I2c`
//! struct, exposes a combination of the `read`, `modify` and `write` methods.
//! Which methods exposes each register depends on whether the register is
//! read-only, read-write or write-only:
//!
//! - read-only registers only expose the `read` method.
//! - write-only registers only expose the `write` method.
//! - read-write registers expose all the methods: `read`, `modify` and
//!   `write`.
//!
//! This is signature of each of these methods:
//!
//! (using the `CR2` register as an example)
//!
//! ``` rust
//! impl Cr2 {
//!     pub fn modify<F>(&mut self, f: F)
//!         where for<'w> F: FnOnce(&Cr2R, &'w mut Cr2W) -> &'w mut Cr2W
//!     {
//!         ..
//!     }
//!
//!     pub fn read(&self) -> Cr2R { .. }
//!
//!     pub fn write<F>(&mut self, f: F)
//!         where F: FnOnce(&mut Cr2W) -> &mut Cr2W,
//!     {
//!         ..
//!     }
//! }
//! ```
//!
//! The `read` method "reads" the register using a **single**, volatile `LDR`
//! instruction and returns a proxy `Cr2R` struct that allows access to only the
//! readable bits (i.e. not to the reserved or write-only bits) of the `CR2`
//! register:
//!
//! ``` rust
//! impl Cr2R {
//!     /// Bit 0 - Slave address bit 0 (master mode)
//!     pub fn sadd0(&self) -> bool { .. }
//!
//!     /// Bits 1:7 - Slave address bit 7:1 (master mode)
//!     pub fn sadd1(&self) -> u8 { .. }
//!
//!     (..)
//! }
//! ```
//!
//! Usage looks like this:
//!
//! ``` rust
//! // is the SADD0 bit of the CR2 register set?
//! if i2c1.c2r.read().sadd0() {
//!     // something
//! } else {
//!     // something else
//! }
//! ```
//!
//! The `write` method writes some value to the register using a **single**,
//! volatile `STR` instruction. This method involves a `Cr2W` struct that only
//! allows constructing valid states of the `CR2` register.
//!
//! The only constructor that `Cr2W` provides is `reset_value` which returns the
//! value of the `CR2` register after a reset. The rest of `Cr2W` methods are
//! "builder-like" and can be used to set or reset the writable bits of the
//! `CR2` register.
//!
//! ``` rust
//! impl Cr2W {
//!     /// Reset value
//!     pub fn reset_value() -> Self {
//!         Cr2W { bits: 0 }
//!     }
//!
//!     /// Bits 1:7 - Slave address bit 7:1 (master mode)
//!     pub fn sadd1(&mut self, value: u8) -> &mut Self { .. }
//!
//!     /// Bit 0 - Slave address bit 0 (master mode)
//!     pub fn sadd0(&mut self, value: bool) -> &mut Self { .. }
//! }
//! ```
//!
//! The `write` method takes a closure with signature `&mut Cr2W -> &mut Cr2W`.
//! If the "identity closure", `|w| w`, is passed then `write` method will set
//! the `CR2` register to its reset value. Otherwise, the closure specifies how
//! that reset value will be modified *before* it's written to `CR2`.
//!
//! Usage looks like this:
//!
//! ``` rust
//! // Write to CR2, its reset value (`0x0000_0000`) but with its SADD0 and
//! // SADD1 fields set to `true` and `0b0011110` respectively
//! i2c1.cr2.write(|w| w.sadd0(true).sadd1(0b0011110));
//! ```
//!
//! Finally, the `modify` method performs a **single** read-modify-write
//! operation that involves reading (`LDR`) the register, modifying the fetched
//! value and then writing (`STR`) the modified value to the register. This
//! method accepts a closure that specifies how the `CR2` register will be
//! modified (the `w` argument) and also provides access to the state of the
//! register before it's modified (the `r` argument).
//!
//! Usage looks like this:
//!
//! ``` rust
//! // Set the START bit to 1 while KEEPING the state of the other bits intact
//! i2c1.cr2.modify(|_, w| w.start(true));
//!
//! // TOGGLE the STOP bit
//! i2c1.cr2.modify(|r, w| w.stop(!r.stop()));
//! ```

#![recursion_limit = "128"]

extern crate either;
extern crate inflections;
extern crate svd_parser as svd;
#[macro_use]
extern crate quote;
extern crate syn;

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Write;
use std::io;
use std::rc::Rc;

use either::Either;
use inflections::Inflect;
use quote::Tokens;
use svd::{Access, BitRange, Defaults, EnumeratedValues, Field, Peripheral,
          Register, RegisterInfo, Usage};
use syn::*;

trait ToSanitizedPascalCase {
    fn to_sanitized_pascal_case(&self) -> Cow<str>;
}

trait ToSanitizedSnakeCase {
    fn to_sanitized_snake_case(&self) -> Cow<str>;
}

impl ToSanitizedSnakeCase for str {
    fn to_sanitized_snake_case(&self) -> Cow<str> {
        macro_rules! keywords {
            ($($kw:ident),+,) => {
                Cow::from(match &self.to_lowercase()[..] {
                    $(stringify!($kw) => concat!(stringify!($kw), "_")),+,
                    _ => return Cow::from(self.to_snake_case())
                })
            }
        }

        match self.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", self.to_snake_case()))
            }
            _ => {
                keywords!{
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
        match self.chars().next().unwrap_or('\0') {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                Cow::from(format!("_{}", self.to_pascal_case()))
            }
            _ => Cow::from(self.to_pascal_case()),
        }
    }
}

#[doc(hidden)]
pub fn gen_peripheral(p: &Peripheral, d: &Defaults) -> Vec<Tokens> {
    assert!(p.derived_from.is_none(),
            "DerivedFrom not supported here (should be resolved earlier)");

    let mut items = vec![];
    let mut fields = vec![];
    let mut offset = 0;
    let mut i = 0;
    let registers = p.registers
        .as_ref()
        .expect(&format!("{:#?} has no `registers` field", p));

    for register in &expand(registers) {
        let pad = if let Some(pad) = register.offset
            .checked_sub(offset) {
            pad
        } else {
            writeln!(io::stderr(),
                     "WARNING {} overlaps with another register at offset \
                      {}. Ignoring.",
                     register.name,
                     register.offset)
                .ok();
            continue;
        };

        if pad != 0 {
            let name = Ident::new(format!("_reserved{}", i));
            let pad = pad as usize;
            fields.push(quote! {
                #name : [u8; #pad]
            });
            i += 1;
        }

        let comment = &format!("0x{:02x} - {}",
                               register.offset,
                               respace(&register.info
                                   .description))
                           [..];

        let reg_ty = match register.ty {
            Either::Left(ref ty) => Ident::from(&**ty),
            Either::Right(ref ty) => Ident::from(&***ty),
        };
        let reg_name = Ident::new(&*register.name.to_sanitized_snake_case());
        fields.push(quote! {
            #[doc = #comment]
            pub #reg_name : #reg_ty
        });

        offset = register.offset +
                 register.info
            .size
            .or(d.size)
            .expect(&format!("{:#?} has no `size` field", register.info)) /
                 8;
    }

    let p_name = Ident::new(&*p.name.to_sanitized_pascal_case());

    let doc = p.description
        .as_ref()
        .map(|s| respace(s))
        .unwrap_or_else(|| "Peripheral".to_owned());
    items.push(quote! {
        #![doc = #doc]

        /// Register block
        #[repr(C)]
        pub struct #p_name {
            #(#fields),*
        }
    });

    for register in registers {
        items.extend(gen_register(register, d, registers));
    }

    items
}

struct ExpandedRegister<'a> {
    info: &'a RegisterInfo,
    name: String,
    offset: u32,
    ty: Either<String, Rc<String>>,
}

/// Takes a list of "registers", some of which may actually be register arrays,
/// and turns it into a new *sorted* (by address offset) list of registers where
/// the register arrays have been expanded.
fn expand(registers: &[Register]) -> Vec<ExpandedRegister> {
    let mut out = vec![];

    for r in registers {
        match *r {
            Register::Single(ref info) => {
                out.push(ExpandedRegister {
                    info: info,
                    name: info.name.to_sanitized_snake_case().into_owned(),
                    offset: info.address_offset,
                    ty: Either::Left(info.name
                        .to_sanitized_pascal_case()
                        .into_owned()),
                })
            }
            Register::Array(ref info, ref array_info) => {
                let has_brackets = info.name.contains("[%s]");

                let ty = if has_brackets {
                    info.name.replace("[%s]", "")
                } else {
                    info.name.replace("%s", "")
                };

                let ty = Rc::new(ty.to_sanitized_pascal_case().into_owned());

                let indices = array_info.dim_index
                    .as_ref()
                    .map(|v| Cow::from(&**v))
                    .unwrap_or_else(|| {
                        Cow::from((0..array_info.dim)
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>())
                    });

                for (idx, i) in indices.iter().zip(0..) {
                    let name = if has_brackets {
                        info.name.replace("[%s]", idx)
                    } else {
                        info.name.replace("%s", idx)
                    };

                    let offset = info.address_offset +
                                 i * array_info.dim_increment;

                    out.push(ExpandedRegister {
                        info: info,
                        name: name.to_sanitized_snake_case().into_owned(),
                        offset: offset,
                        ty: Either::Right(ty.clone()),
                    });
                }
            }
        }
    }

    out.sort_by_key(|x| x.offset);

    out
}

fn name_of(r: &Register) -> Cow<str> {
    match *r {
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

fn access(r: &Register) -> Access {
    r.access.unwrap_or_else(|| if let Some(ref fields) = r.fields {
        if fields.iter().all(|f| f.access == Some(Access::ReadOnly)) {
            Access::ReadOnly
        } else if fields.iter().all(|f| f.access == Some(Access::WriteOnly)) {
            Access::WriteOnly
        } else {
            Access::ReadWrite
        }
    } else {
        Access::ReadWrite
    })
}

#[cfg_attr(feature = "cargo-clippy", allow(cyclomatic_complexity))]
#[doc(hidden)]
pub fn gen_register(r: &Register,
                    d: &Defaults,
                    all_registers: &[Register])
                    -> Vec<Tokens> {
    let mut items = vec![];

    let name = name_of(r);
    let name_pc = Ident::new(&*name.to_sanitized_pascal_case());
    let name_sc = Ident::new(&*name.to_sanitized_snake_case());

    let reg_ty = r.size
        .or(d.size)
        .expect(&format!("{:#?} has no `size` field", r))
        .to_ty();
    let access = access(r);

    let doc = respace(&r.description);
    match access {
        Access::ReadOnly => {
            items.push(quote! {
                #[doc = #doc]
                #[repr(C)]
                pub struct #name_pc {
                    register: ::volatile_register::RO<#reg_ty>
                }
            });
        }
        Access::ReadWrite => {
            items.push(quote! {
                #[doc = #doc]
                #[repr(C)]
                pub struct #name_pc {
                    register: ::volatile_register::RW<#reg_ty>
                }
            });
        }
        Access::WriteOnly => {
            items.push(quote! {
                #[doc = #doc]
                #[repr(C)]
                pub struct #name_pc {
                    register: ::volatile_register::WO<#reg_ty>
                }
            });
        }
        _ => unreachable!(),
    }

    let mut mod_items = vec![];
    let mut impl_items = vec![];
    let mut r_impl_items = vec![];
    let mut w_impl_items = vec![];
    if access == Access::ReadWrite {
        impl_items.push(quote! {
            /// Modifies the contents of the register
            pub fn modify<F>(&mut self, f: F)
                where for<'w> F: FnOnce(&R, &'w mut W) -> &'w mut W,
            {
                let bits = self.register.read();
                let r = R { bits: bits };
                let mut w = W { bits: bits };
                f(&r, &mut w);
                self.register.write(w.bits);
            }
        });
    }

    if access == Access::ReadOnly || access == Access::ReadWrite {
        impl_items.push(quote! {
            /// Reads the contents of the register
            pub fn read(&self) -> R {
                R { bits: self.register.read() }
            }
        });

        mod_items.push(quote! {
            /// Value read from the register
            pub struct R {
                bits: #reg_ty,
            }
        });

        r_impl_items.push(quote! {
            /// Value of the register as raw bits
            pub fn bits(&self) -> #reg_ty {
                self.bits
            }
        });
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {
        impl_items.push(quote! {
            /// Writes to the register
            pub fn write<F>(&mut self, f: F)
                where F: FnOnce(&mut W) -> &mut W,
            {
                let mut w = W::reset_value();
                f(&mut w);
                self.register.write(w.bits);
            }
        });

        mod_items.push(quote! {
            /// Value to write to the register
            pub struct W {
                bits: #reg_ty,
            }
        });

        if let Some(reset_value) =
            r.reset_value
                .or(d.reset_value)
                .map(|x| Lit::Int(x as u64, IntTy::Unsuffixed)) {
            w_impl_items.push(quote! {
                /// Reset value of the register
                pub fn reset_value() -> W {
                    W { bits: #reset_value }
                }
            });
        }

        w_impl_items.push(quote! {
            /// Writes raw `bits` to the register
            pub unsafe fn bits(&mut self, bits: #reg_ty) -> &mut Self {
                self.bits = bits;
                self
            }
        });
    }

    mod_items.push(quote! {
        impl super::#name_pc {
            #(#impl_items)*
        }
    });

    let fields = r.fields.as_ref().map(|fs| &**fs).unwrap_or(&[]);

    if !fields.is_empty() {
        let mut reexported = HashSet::new();

        if access == Access::ReadOnly || access == Access::ReadWrite {
            for field in fields {
                if field.access == Some(Access::WriteOnly) {
                    continue;
                }

                let field_name = Ident::new(&*field.name
                    .to_sanitized_snake_case());
                let _field_name = Ident::new(&*format!("_{}",
                                                       field.name
                                                           .to_snake_case()));
                let width = field.bit_range.width;
                let mask = Lit::Int((1u64 << width) - 1, IntTy::Unsuffixed);
                let offset = Lit::Int(u64::from(field.bit_range.offset),
                                      IntTy::Unsuffixed);
                let field_ty = width.to_ty();

                r_impl_items.push(quote! {
                    fn #_field_name(&self) -> #field_ty {
                        const MASK: #field_ty = #mask;
                        const OFFSET: u8 = #offset;

                        ((self.bits >> OFFSET) & MASK as #reg_ty) as #field_ty
                    }
                });

                if let Some((evs, base)) =
                    lookup(&field.enumerated_values,
                           fields,
                           all_registers,
                           Usage::Read) {
                    struct Variant {
                        doc: Cow<'static, str>,
                        pc: Ident,
                        // `None` indicates a reserved variant
                        sc: Option<Ident>,
                        value: u64,
                    }

                    let variants = (0..1 << width)
                        .map(|i| {
                            let value = u64::from(i);
                            if let Some(ev) = evs.values
                                .iter()
                                .find(|ev| ev.value == Some(i)) {
                                let sc = Ident::new(&*ev.name.to_snake_case());
                                let doc = Cow::from(ev.description
                                    .clone()
                                    .unwrap_or_else(|| {
                                        format!("A possible value of \
                                                 the field `{}`",
                                                sc)
                                    }));

                                Variant {
                                    doc: doc,
                                    pc: Ident::new(&*ev.name
                                        .to_sanitized_pascal_case()),
                                    sc: Some(sc),
                                    value: value,
                                }
                            } else {
                                Variant {
                                    doc: Cow::from("Reserved"),
                                    pc: Ident::new(format!("_Reserved{:b}", i)),
                                    sc: None,
                                    value: value,
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    let variants_pc = variants.iter().map(|v| &v.pc);

                    let enum_name = if let Some(ref base) = base {
                        Ident::new(&*format!("{}R",
                                             base.field
                                                 .to_sanitized_pascal_case()))
                    } else {
                        Ident::new(&*format!("{}R",
                                             evs.name
                                                 .as_ref()
                                                 .unwrap_or(&field.name)
                                                 .to_sanitized_pascal_case()))
                    };

                    if let Some(register) = base.as_ref()
                        .and_then(|base| base.register) {
                        let register =
                            Ident::new(&*register.to_sanitized_snake_case());

                        if !reexported.contains(&enum_name) {
                            mod_items.push(quote! {
                                pub use super::#register::#enum_name;
                            });

                            reexported.insert(enum_name.clone());
                        }
                    }

                    let doc = field_doc(field.bit_range,
                                        field.description.as_ref());
                    r_impl_items.push(quote! {
                        #[doc = #doc]
                        pub fn #field_name(&self) -> #enum_name {
                            #enum_name::_from(self.#_field_name())
                        }
                    });

                    if base.is_none() {
                        let doc = format!("Possible values of the field `{}`",
                                          field_name);
                        let variants_doc = variants.iter().map(|v| &*v.doc);
                        mod_items.push(quote! {
                            #[doc = #doc]
                            #[derive(Clone, Copy, Debug, PartialEq)]
                            pub enum #enum_name {
                                #(#[doc = #variants_doc]
                                  #variants_pc),*
                            }
                        });

                        let mut enum_items = vec![];

                        let arms = variants.iter()
                            .map(|v| {
                                let value = Lit::Int(v.value,
                                                     IntTy::Unsuffixed);
                                let pc = &v.pc;

                                quote! {
                                    #enum_name::#pc => #value
                                }
                            });
                        enum_items.push(quote! {
                            /// Value of the field as raw bits
                            pub fn bits(&self) -> #field_ty {
                                match *self {
                                    #(#arms),*
                                }
                            }
                        });

                        let arms = variants.iter()
                            .map(|v| {
                                let i = Lit::Int(v.value, IntTy::Unsuffixed);
                                let pc = &v.pc;

                                quote! {
                                    #i => #enum_name::#pc
                                }
                            });

                        enum_items.push(quote! {
                            #[allow(missing_docs)]
                            #[doc(hidden)]
                            #[inline(always)]
                            pub fn _from(bits: #field_ty) -> #enum_name {
                                match bits {
                                    #(#arms),*,
                                    _ => unreachable!(),
                                }
                            }
                        });

                        for v in &variants {
                            if let Some(ref sc) = v.sc {
                                let pc = &v.pc;

                                let is_variant = {
                                    Ident::new(&*format!("is_{}", sc))
                                };

                                let doc = format!("Check if \
                                                   the value of the field \
                                                   is `{}`",
                                                  pc);
                                enum_items.push(quote! {
                                    #[doc = #doc]
                                    pub fn #is_variant(&self) -> bool {
                                        *self == #enum_name::#pc
                                    }
                                });
                            }
                        }

                        mod_items.push(quote! {
                            impl #enum_name {
                                #(#enum_items)*
                            }
                        });
                    }
                } else {
                    let name = Ident::new(&*format!("{}R",
                                            field.name
                                            .to_sanitized_pascal_case()));
                    let doc = format!("Value of the field {}", field.name);
                    mod_items.push(quote! {
                        #[doc = #doc]
                        pub struct #name {
                            bits: #field_ty,
                        }

                        impl #name {
                            /// Value of the field as raw bits
                            pub fn bits(&self) -> #field_ty {
                                self.bits
                            }
                        }
                    });

                    let doc = field_doc(field.bit_range,
                                        field.description.as_ref());
                    r_impl_items.push(quote! {
                        #[doc = #doc]
                        pub fn #field_name(&self) -> #name {
                            #name { bits: self.#_field_name() }
                        }
                    });
                }
            }
        }

        if access == Access::WriteOnly || access == Access::ReadWrite {
            for field in fields {
                if field.access == Some(Access::ReadOnly) {
                    continue;
                }

                let field_name_sc = Ident::new(&*field.name
                    .to_sanitized_snake_case());
                let width = field.bit_range.width;
                let mask = Lit::Int((1u64 << width) - 1, IntTy::Unsuffixed);
                let offset = Lit::Int(u64::from(field.bit_range.offset),
                                      IntTy::Unsuffixed);
                let field_ty = width.to_ty();
                let proxy = Ident::new(&*format!("_{}W",
                                                 field.name
                                                     .to_pascal_case()));

                mod_items.push(quote! {
                    /// Proxy
                    pub struct #proxy<'a> {
                        register: &'a mut W,
                    }
                });

                let mut proxy_items = vec![];

                let mut bits_is_safe = false;
                if let Some((evs, base)) =
                    lookup(&field.enumerated_values,
                           fields,
                           all_registers,
                           Usage::Write) {
                    struct Variant {
                        doc: String,
                        pc: Ident,
                        sc: Ident,
                        value: u64,
                    }

                    let enum_name = if let Some(ref base) = base {
                        Ident::new(&*format!("{}W",
                                             base.field
                                                 .to_sanitized_pascal_case()))
                    } else {
                        Ident::new(&*format!("{}W",
                                             evs.name
                                                 .as_ref()
                                                 .unwrap_or(&field.name)
                                                 .to_sanitized_pascal_case()))
                    };

                    if let Some(register) = base.as_ref()
                        .and_then(|base| base.register) {
                        let register =
                            Ident::new(&*register.to_sanitized_snake_case());

                        if !reexported.contains(&enum_name) {
                            mod_items.push(quote! {
                                pub use super::#register::#enum_name;
                            });

                            reexported.insert(enum_name.clone());
                        }
                    }

                    let variants =
                        evs.values
                            .iter()
                            .map(|ev| {
                                // TODO better error message
                                let value = u64::from(ev.value
                                    .expect("no value in EnumeratedValue"));

                                Variant {
                                    doc: ev.description
                                        .clone()
                                        .unwrap_or_else(|| {
                                            format!("`{:b}`", value)
                                        }),
                                    pc: Ident::new(&*ev.name
                                        .to_sanitized_pascal_case()),
                                    sc: Ident::new(&*ev.name
                                        .to_sanitized_snake_case()),
                                    value: value,
                                }
                            })
                            .collect::<Vec<_>>();

                    // Whether the `bits` method should be `unsafe`.
                    // `bits` can be safe when enumeratedValues covers all
                    // the possible values of the bitfield or, IOW, when
                    // there are no reserved bit patterns.
                    bits_is_safe = variants.len() == 1 << width;

                    if base.is_none() {
                        let variants_pc = variants.iter().map(|v| &v.pc);
                        let doc = {
                            format!("Values that can be written \
                                     to the field `{}`",
                                    field_name_sc)
                        };
                        let variants_doc = variants.iter().map(|v| &*v.doc);
                        mod_items.push(quote! {
                            #[doc = #doc]
                            pub enum #enum_name {
                                #(#[doc = #variants_doc]
                                  #variants_pc),*
                            }
                        });

                        let arms = variants.iter()
                            .map(|v| {
                                let pc = &v.pc;
                                let value = Lit::Int(v.value,
                                                     IntTy::Unsuffixed);

                                quote! {
                                    #enum_name::#pc => #value
                                }
                            });

                        mod_items.push(quote! {
                            impl #enum_name {
                                #[allow(missing_docs)]
                                #[doc(hidden)]
                                #[inline(always)]
                                pub fn _bits(&self) -> #field_ty {
                                    match *self {
                                        #(#arms),*
                                    }
                                }
                            }
                        });
                    }

                    if bits_is_safe {
                        proxy_items.push(quote! {
                            /// Writes `variant` to the field
                            pub fn variant(self,
                                        variant: #enum_name) -> &'a mut W {
                                self.bits(variant._bits())
                            }
                        });
                    } else {
                        proxy_items.push(quote! {
                            /// Writes `variant` to the field
                            pub fn variant(self,
                                        variant: #enum_name) -> &'a mut W {
                                unsafe {
                                    self.bits(variant._bits())
                                }
                            }
                        });
                    }

                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.sc;

                        let doc = respace(&v.doc);
                        proxy_items.push(quote! {
                            #[doc = #doc]
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#enum_name::#pc)
                            }
                        });
                    }
                }

                if bits_is_safe {
                    proxy_items.push(quote! {
                        /// Writes raw `bits` to the field
                        pub fn bits(self, bits: #field_ty) -> &'a mut W {
                            const MASK: #field_ty = #mask;
                            const OFFSET: u8 = #offset;

                            self.register.bits &=
                                !((MASK as #reg_ty) << OFFSET);
                            self.register.bits |=
                                ((bits & MASK) as #reg_ty) << OFFSET;
                            self.register
                        }
                    });
                } else {
                    proxy_items.push(quote! {
                        /// Writes raw `bits` to the field
                        pub unsafe fn bits(self,
                                            bits: #field_ty) -> &'a mut W {
                            const MASK: #field_ty = #mask;
                            const OFFSET: u8 = #offset;

                            self.register.bits &=
                                !((MASK as #reg_ty) << OFFSET);
                            self.register.bits |=
                                ((bits & MASK) as #reg_ty) << OFFSET;
                            self.register
                        }
                    });
                }


                mod_items.push(quote! {
                    impl<'a> #proxy<'a> {
                        #(#proxy_items)*
                    }
                });

                let doc = field_doc(field.bit_range,
                                    field.description.as_ref());
                w_impl_items.push(quote! {
                    #[doc = #doc]
                    pub fn #field_name_sc(&mut self) -> #proxy {
                        #proxy {
                            register: self,
                        }
                    }
                });
            }
        }
    }

    if access == Access::ReadOnly || access == Access::ReadWrite {
        mod_items.push(quote! {
            impl R {
                #(#r_impl_items)*
            }
        });
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {
        mod_items.push(quote! {
            impl W {
                #(#w_impl_items)*
            }
        });
    }

    let doc = respace(&r.description);
    items.push(quote! {
        #[doc = #doc]
        pub mod #name_sc {
            #(#mod_items)*
        }
    });

    items
}

fn lookup<'a>(evs: &'a [EnumeratedValues],
              fields: &'a [Field],
              all_registers: &'a [Register],
              usage: Usage)
              -> Option<(&'a EnumeratedValues, Option<Base<'a>>)> {
    match evs.first() {
            Some(head) if evs.len() == 1 => Some(head),
            None => None,
            _ => evs.iter().find(|ev| ev.usage == Some(usage)),
        }
        .map(|evs| {
            if let Some(ref base) = evs.derived_from {
                let mut parts = base.split('.');

                let (register, fields, field) = match (parts.next(),
                                                       parts.next()) {
                    (Some(register), Some(field)) => {
                        // TODO better error message
                        let fields = all_registers.iter()
                            .find(|r| r.name == register)
                            .expect("couldn't find register")
                            .fields
                            .as_ref()
                            .expect("no fields");

                        (Some(register), &fields[..], field)
                    }
                    (Some(field), None) => (None, fields, field),
                    _ => unreachable!(),
                };

                // TODO better error message
                let evs = fields.iter()
                    .flat_map(|f| f.enumerated_values.iter())
                    .find(|evs| evs.name.as_ref().map(|s| &**s) == Some(field))
                    .expect("");

                (evs,
                 Some(Base {
                     register: register,
                     field: field,
                 }))
            } else {
                (evs, None)
            }
        })
}

fn field_doc(bit_range: BitRange, doc: Option<&String>) -> String {
    let BitRange { offset, width } = bit_range;

    if let Some(doc) = doc {
        let doc = respace(doc);

        if width == 1 {
            format!("Bit {} - {}", offset, doc)
        } else {
            format!("Bits {}:{} - {}", offset, offset + width - 1, doc)
        }
    } else if width == 1 {
        format!("Bit {}", offset)
    } else {
        format!("Bits {}:{}", offset, offset + width - 1)
    }
}

struct Base<'a> {
    register: Option<&'a str>,
    field: &'a str,
}

trait U32Ext {
    fn to_ty(&self) -> Ident;
}

impl U32Ext for u32 {
    fn to_ty(&self) -> Ident {
        match *self {
            1...8 => Ident::new("u8"),
            9...16 => Ident::new("u16"),
            17...32 => Ident::new("u32"),
            _ => panic!("{}.to_ty()", *self),
        }
    }
}

fn respace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
