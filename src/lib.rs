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
use std::io::Write;
use std::io;
use std::rc::Rc;

use either::Either;
use inflections::Inflect;
use quote::Tokens;
use svd::{Access, Defaults, Peripheral, Register, RegisterInfo};
use syn::*;

/// Trait that sanitizes name avoiding rust keywords and the like.
trait SanitizeName {
    /// Sanitize a name; avoiding Rust keywords and the like.
    fn sanitize(self) -> String;
}

impl SanitizeName for String {
    fn sanitize(self) -> String {
        const KEYWORDS: [&'static str; 52] =
            ["abstract", "alignof", "as", "become", "box", "break", "const", "continue", "crate",
             "do", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in",
             "let", "loop", "macro", "match", "mod", "move", "mut", "offsetof", "override",
             "priv", "proc", "pub", "pure", "ref", "return", "Self", "self", "sizeof", "static",
             "struct", "super", "trait", "true", "type", "typeof", "unsafe", "unsized", "use",
             "virtual", "where", "while", "yield"];

        if KEYWORDS.contains(&self.as_str()) {
            self + "_"
        } else {
            self
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

    for register in expand(registers).iter() {
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
                                   .description))[..];

        let reg_ty = match register.ty {
            Either::Left(ref ty) => Ident::from(&**ty),
            Either::Right(ref ty) => Ident::from(&***ty),
        };
        let reg_name = Ident::new(&*register.name);
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

    let p_name = Ident::new(p.name.to_pascal_case());

    if let Some(description) = p.description.as_ref() {
        let comment = &respace(description)[..];
        items.push(quote! {
            #[doc = #comment]
        });
    }

    let struct_ = quote! {
        #[repr(C)]
        pub struct #p_name {
            #(#fields),*
        }
    };

    items.push(struct_);

    for register in registers {
        let access = access(&register);

        items.extend(gen_register(register, d));
        if let Some(ref fields) = register.fields {
            if access != Access::WriteOnly {
                items.extend(gen_register_r(register, d, fields));
            }
            if access != Access::ReadOnly {
                items.extend(gen_register_w(register, d, fields));
            }
        }
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
                    name: info.name.to_snake_case().sanitize(),
                    offset: info.address_offset,
                    ty: Either::Left(info.name.to_pascal_case()),
                })
            }
            Register::Array(ref info, ref array_info) => {
                let has_brackets = info.name.contains("[%s]");

                let ty = if has_brackets {
                    info.name.replace("[%s]", "")
                } else {
                    info.name.replace("%s", "")
                };

                let ty = Rc::new(ty.to_pascal_case());

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
                        name: name.to_snake_case().sanitize(),
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

fn type_of(r: &Register) -> String {
    let ty = match *r {
        Register::Single(ref info) => Cow::from(&*info.name),
        Register::Array(ref info, _) => {
            if info.name.contains("[%s]") {
                info.name.replace("[%s]", "").into()
            } else {
                info.name.replace("%s", "").into()
            }
        }
    };

    (&*ty).to_pascal_case()
}

fn access(r: &Register) -> Access {
    r.access.unwrap_or_else(|| {
        if let Some(ref fields) = r.fields {
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

#[doc(hidden)]
pub fn gen_register(r: &Register, d: &Defaults) -> Vec<Tokens> {
    let mut items = vec![];

    let ty = type_of(r);
    let name = Ident::new(&*ty);
    let bits_ty = r.size
        .or(d.size)
        .expect(&format!("{:#?} has no `size` field", r))
        .to_ty();
    let access = access(r);

    match access {
        Access::ReadOnly => {
            items.push(quote! {
                #[repr(C)]
                pub struct #name {
                    register: ::volatile_register::RO<#bits_ty>
                }
            });
        }
        Access::ReadWrite => {
            items.push(quote! {
                #[repr(C)]
                pub struct #name {
                    register: ::volatile_register::RW<#bits_ty>
                }
            });
        }
        Access::WriteOnly => {
            items.push(quote! {
                #[repr(C)]
                pub struct #name {
                    register: ::volatile_register::WO<#bits_ty>
                }
            });
        }
        _ => unreachable!(),
    }

    if r.fields.is_some() {
        let name_r = Ident::new(format!("{}R", ty));
        let name_w = Ident::new(format!("{}W", ty));
        match access {
            Access::ReadOnly => {
                items.push(quote! {
                    impl #name {
                        pub fn read_bits(&self) -> #bits_ty {
                            self.register.read()
                        }

                        pub fn read(&self) -> #name_r {
                            #name_r { bits: self.register.read() }
                        }
                    }
                });
            }
            Access::ReadWrite => {
                items.push(quote! {
                    impl #name {
                        pub fn read_bits(&self) -> #bits_ty {
                            self.register.read()
                        }

                        pub unsafe fn modify_bits<F>(&mut self, f: F)
                            where F: FnOnce(&mut #bits_ty)
                        {
                            let mut bits = self.register.read();
                            f(&mut bits);
                            self.register.write(bits);
                        }

                        pub unsafe fn write_bits(&mut self, bits: #bits_ty) {
                            self.register.write(bits);
                        }

                        pub fn modify<F>(&mut self, f: F)
                            where for<'w> F: FnOnce(&#name_r, &'w mut #name_w) -> &'w mut #name_w,
                        {
                            let bits = self.register.read();
                            let r = #name_r { bits: bits };
                            let mut w = #name_w { bits: bits };
                            f(&r, &mut w);
                            self.register.write(w.bits);
                        }

                        pub fn read(&self) -> #name_r {
                            #name_r { bits: self.register.read() }
                        }

                        pub fn write<F>(&mut self, f: F)
                            where F: FnOnce(&mut #name_w) -> &mut #name_w,
                        {
                            let mut w = #name_w::reset_value();
                            f(&mut w);
                            self.register.write(w.bits);
                        }
                    }
                });
            }

            Access::WriteOnly => {
                items.push(quote! {
                    impl #name {
                        pub unsafe fn write_bits(&mut self, bits: #bits_ty) {
                            self.register.write(bits);
                        }

                        pub fn write<F>(&self, f: F)
                            where F: FnOnce(&mut #name_w) -> &mut #name_w,
                        {
                            let mut w = #name_w::reset_value();
                            f(&mut w);
                            self.register.write(w.bits);
                        }
                    }
                });
            }

            _ => unreachable!(),
        }
    } else {
        match access {
            Access::ReadOnly => {
                items.push(quote! {
                    impl #name {
                        pub fn read(&self) -> #bits_ty {
                            self.register.read()
                        }
                    }
                });
            }
            Access::ReadWrite => {
                items.push(quote! {
                    impl #name {
                        pub fn read(&self) -> #bits_ty {
                            self.register.read()
                        }

                        pub fn write(&mut self, value: #bits_ty) {
                            self.register.write(value);
                        }
                    }
                });
            }

            Access::WriteOnly => {
                items.push(quote! {
                    impl #name {
                        pub fn write(&mut self, value: #bits_ty) {
                            self.register.write(value);
                        }
                    }
                });
            }

            _ => unreachable!(),
        }
    }

    items
}

#[doc(hidden)]
pub fn gen_register_r(r: &Register,
                      d: &Defaults,
                      fields: &[svd::Field])
                      -> Vec<Tokens> {
    let mut items = vec![];

    let name = Ident::new(format!("{}R", type_of(r)));
    let bits_ty = r.size
        .or(d.size)
        .expect(&format!("{:#?} has no `size` field", r))
        .to_ty();

    items.push(quote! {
        #[derive(Clone, Copy)]
        #[repr(C)]
        pub struct #name {
            bits: #bits_ty,
        }});

    let mut impl_items = vec![];

    for field in fields {
        // Skip fields named RESERVED because, well, they are reserved so they
        // shouldn't be modified/exposed
        if field.name.to_lowercase() == "reserved" {
            continue;
        }

        if let Some(Access::WriteOnly) = field.access {
            continue;
        }

        let name = Ident::new(field.name.to_snake_case().sanitize());
        let offset = field.bit_range.offset as u8;

        let width = field.bit_range.width;

        if let Some(description) = field.description.as_ref() {
            let bits = if width == 1 {
                format!("Bit {}", field.bit_range.offset)
            } else {
                format!("Bits {}:{}",
                        field.bit_range.offset,
                        field.bit_range.offset + width - 1)
            };

            let comment = &format!("{} - {}", bits, respace(description))[..];
            impl_items.push(quote! {
                #[doc = #comment]
            });
        }

        let item = if width == 1 {
            quote! {
                pub fn #name(&self) -> bool {
                    const OFFSET: u8 = #offset;

                    self.bits & (1 << OFFSET) != 0
                }
            }
        } else {
            let width_ty = width.to_ty();
            let mask: u64 = (1 << width) - 1;
            let mask = Lit::Int(mask, IntTy::Unsuffixed);

            quote! {
                pub fn #name(&self) -> #width_ty {
                    const MASK: #bits_ty = #mask;
                    const OFFSET: u8 = #offset;

                    ((self.bits >> OFFSET) & MASK) as #width_ty
                }
            }
        };

        impl_items.push(item);
    }

    items.push(quote! {
        impl #name {
            #(#impl_items)*
        }
    });

    items
}

#[doc(hidden)]
pub fn gen_register_w(r: &Register,
                      d: &Defaults,
                      fields: &[svd::Field])
                      -> Vec<Tokens> {
    let mut items = vec![];

    let name = Ident::new(format!("{}W", type_of(r)));
    let bits_ty = r.size
        .or(d.size)
        .expect(&format!("{:#?} has no `size` field", r))
        .to_ty();
    items.push(quote! {
        #[derive(Clone, Copy)]
        #[repr(C)]
        pub struct #name {
            bits: #bits_ty,
        }
    });

    let mut impl_items = vec![];

    if let Some(reset_value) =
        r.reset_value
            .or(d.reset_value)
            .map(|x| Lit::Int(x as u64, IntTy::Unsuffixed)) {
        impl_items.push(quote! {
            /// Reset value
            pub fn reset_value() -> Self {
                #name { bits: #reset_value }
            }
        });
    }

    for field in fields {
        // Skip fields named RESERVED. See `gen_register_r` for an explanation
        if field.name.to_lowercase() == "reserved" {
            continue;
        }

        if let Some(Access::ReadOnly) = field.access {
            continue;
        }

        let name = Ident::new(field.name.to_snake_case().sanitize());
        let offset = field.bit_range.offset as u8;

        let width = field.bit_range.width;

        if let Some(description) = field.description.as_ref() {
            let bits = if width == 1 {
                format!("Bit {}", field.bit_range.offset)
            } else {
                format!("Bits {}:{}",
                        field.bit_range.offset,
                        field.bit_range.offset + width - 1)
            };

            let comment = &format!("{} - {}", bits, respace(description))[..];
            impl_items.push(quote! {
                #[doc = #comment]
            });
        }

        let item = if width == 1 {
            quote! {
                pub fn #name(&mut self, value: bool) -> &mut Self {
                    const OFFSET: u8 = #offset;

                    if value {
                        self.bits |= 1 << OFFSET;
                    } else {
                        self.bits &= !(1 << OFFSET);
                    }
                    self
                }
            }
        } else {
            let width_ty = width.to_ty();
            let mask = (1 << width) - 1;
            let mask = Lit::Int(mask, IntTy::Unsuffixed);

            quote! {
                pub fn #name(&mut self, value: #width_ty) -> &mut Self {
                    const OFFSET: u8 = #offset;
                    const MASK: #width_ty = #mask;

                    self.bits &= !((MASK as #bits_ty) << OFFSET);
                    self.bits |= ((value & MASK) as #bits_ty) << OFFSET;
                    self
                }
            }
        };

        impl_items.push(item);
    }

    items.push(quote! {
        impl #name {
            #(#impl_items)*
        }
    });

    items
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
