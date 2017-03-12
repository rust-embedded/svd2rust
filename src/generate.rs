use std::collections::HashMap;
use std::io::{self, Write};

use cast::u64;
use either::Either;
use quote::Tokens;
use svd::{Access, BitRange, Defaults, Device, EnumeratedValues, Field,
          Peripheral, Register, Usage};
use syn::{Ident, Lit};

use errors::*;
use util::{self, ToSanitizedPascalCase, ToSanitizedSnakeCase, U32Ext};

/// Whole device generation
pub fn device(d: &Device, items: &mut Vec<Tokens>) -> Result<()> {
    let doc = format!("Peripheral access API for {} microcontrollers \
                       (generated using svd2rust v{})",
                      d.name.to_uppercase(),
                      env!("CARGO_PKG_VERSION"));
    items.push(quote! {
        #![doc = #doc]
        #![deny(missing_docs)]
        #![deny(warnings)]
        #![feature(const_fn)]
        #![no_std]

        extern crate cortex_m;
        extern crate vcell;

        use cortex_m::peripheral::Peripheral;
    });

    ::generate::interrupt(&d.peripherals, items);

    for p in &d.peripherals {
        ::generate::peripheral(p, items, &d.defaults)?;
    }

    Ok(())
}

/// Generates code for `src/interrupt.rs`
pub fn interrupt(peripherals: &[Peripheral], items: &mut Vec<Tokens>) {
    let interrupts = peripherals.iter()
        .flat_map(|p| p.interrupt.iter())
        .map(|i| (i.value, i))
        .collect::<HashMap<_, _>>();

    let interrupts = interrupts.into_iter().map(|(_, v)| v).collect::<Vec<_>>();

    let mut fields = vec![];
    let mut exprs = vec![];
    let mut variants = vec![];
    let mut arms = vec![];

    // Current position in the vector table
    let mut pos = 0;
    // Counter for reserved blocks
    let mut res = 0;
    let mut uses_reserved = false;
    let mut mod_items = vec![];
    mod_items.push(quote! {
        use cortex_m::ctxt::Context;
        use cortex_m::exception;
        use cortex_m::interrupt::Nr;
    });
    for interrupt in &interrupts {
        if pos < interrupt.value {
            let name = Ident::new(&*format!("_reserved{}", res));
            res += 1;
            let n = util::unsuffixed(u64(interrupt.value - pos));

            uses_reserved = true;
            fields.push(quote! {
                /// Reserved spot in the vector table
                pub #name: [Reserved; #n],
            });

            exprs.push(quote! {
                #name: [Reserved::Vector; #n],
            });
        }

        let name = Ident::new(&*interrupt.name.to_sanitized_snake_case());
        let name_pc = Ident::new(interrupt.name.to_sanitized_pascal_case());
        let description = interrupt.description
            .as_ref()
            .map(|s| util::respace(s))
            .unwrap_or_else(|| interrupt.name.clone());
        fields.push(quote! {
            #[doc = #description]
            pub #name: unsafe extern "C" fn(#name_pc),
        });

        mod_items.push(quote! {
            #[doc = #description]
            pub struct #name_pc { _0: () }
            unsafe impl Context for #name_pc {}
        });

        exprs.push(quote! {
            #name: exception::default_handler,
        });

        variants.push(quote! {
            #[doc = #description]
            #name_pc,
        });

        let value = util::unsuffixed(u64(interrupt.value));
        arms.push(quote! {
            Interrupt::#name_pc => #value,
        });

        pos = interrupt.value + 1;
    }

    if uses_reserved {
        mod_items.push(quote! {
            use cortex_m::Reserved;
        });
    }

    mod_items.push(quote! {
        /// Interrupt handlers
        #[repr(C)]
        pub struct Handlers {
            #(#fields)*
        }

        /// Default interrupt handlers
        pub const DEFAULT_HANDLERS: Handlers = Handlers {
            #(#exprs)*
        };

        /// Enumeration of all the interrupts
        pub enum Interrupt {
            #(#variants)*
        }

        unsafe impl Nr for Interrupt {
            #[inline(always)]
            fn nr(&self) -> u8 {
                match *self {
                    #(#arms)*
                }
            }
        }
    });

    items.push(quote! {
        /// Interrupts
        pub mod interrupt {
            #(#mod_items)*
        }
    });
}

pub fn peripheral(
    p: &Peripheral,
    items: &mut Vec<Tokens>,
    defaults: &Defaults,
) -> Result<()> {
    let name = Ident::new(&*p.name.to_uppercase());
    let name_pc = Ident::new(&*p.name.to_sanitized_pascal_case());
    let address = util::unsuffixed(u64(p.base_address));
    let description =
        util::respace(p.description.as_ref().unwrap_or(&p.name));

    items.push(quote! {
        #[doc = #description]
        pub const #name: Peripheral<#name_pc> =
            unsafe { Peripheral::new(#address) };
    });

    if let Some(base) = p.derived_from.as_ref() {
        // TODO Verify that base exists
        let base_pc = Ident::new(&*base.to_sanitized_pascal_case());
        items.push(quote! {
            /// Register block
            pub type #name_pc = #base_pc;
        });

        // TODO We don't handle inheritance style `derivedFrom`, we should raise
        // an error in that case
        return Ok(());
    }

    let registers = p.registers
        .as_ref()
        .ok_or_else(|| {
                        format!("Peripheral {} has no <registers> fields",
                                p.name)
                    })?;

    // No `struct RegisterBlock` can be generated
    if registers.is_empty() {
        // Drop the `pub const` definition of the peripheral
        items.pop();
        return Ok(());
    }

    let mut mod_items = vec![];
    mod_items.push(::generate::register_block(registers, defaults)?);

    for register in registers {
        ::generate::register(register, registers, p, defaults, &mut mod_items)?;
    }

    let name_sc = Ident::new(&*p.name.to_sanitized_snake_case());
    let description =
        util::respace(p.description.as_ref().unwrap_or(&p.name));
    items.push(quote! {
        #[doc = #description]
        pub mod #name_sc {
            use vcell::VolatileCell;

            #(#mod_items)*
        }

        pub use #name_sc::RegisterBlock as #name_pc;
    });

    Ok(())
}

fn register_block(registers: &[Register], defs: &Defaults) -> Result<Tokens> {
    let mut fields = vec![];
    // enumeration of reserved fields
    let mut i = 0;
    // offset from the base address, in bytes
    let mut offset = 0;
    for register in util::expand(registers) {
        let pad = if let Some(pad) = register.offset.checked_sub(offset) {
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
                #name : [u8; #pad],
            });
            i += 1;
        }

        let comment = &format!("0x{:02x} - {}",
                               register.offset,
                               util::respace(&register.info.description))
                           [..];

        let rty = match register.ty {
            Either::Left(ref ty) => Ident::from(&**ty),
            Either::Right(ref ty) => Ident::from(&***ty),
        };
        let reg_name = Ident::new(&*register.name.to_sanitized_snake_case());
        fields.push(quote! {
            #[doc = #comment]
            pub #reg_name : #rty,
        });

        offset = register.offset +
                 register.info
                     .size
                     .or(defs.size)
                     .ok_or_else(|| {
                                     format!("Register {} has no `size` field",
                                             register.name)
                                 })? / 8;
    }

    Ok(quote! {
        /// Register block
        pub struct RegisterBlock {
            #(#fields)*
        }
    })
}

pub fn register(
    register: &Register,
    all_registers: &[Register],
    peripheral: &Peripheral,
    defs: &Defaults,
    items: &mut Vec<Tokens>,
) -> Result<()> {
    let access = util::access_of(register);
    let name = util::name_of(register);
    let name_pc = Ident::new(&*name.to_sanitized_pascal_case());
    let name_sc = Ident::new(&*name.to_sanitized_snake_case());
    let rty = register.size
        .or(defs.size)
        .ok_or_else(|| {
                        format!("Register {} has no `size` field",
                                register.name)
                    })?
        .to_ty()?;
    let description = util::respace(&register.description);

    let mut mod_items = vec![];
    let mut reg_impl_items = vec![];
    let mut r_impl_items = vec![];
    let mut w_impl_items = vec![];

    if access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Modifies the contents of the register
            #[inline(always)]
            pub fn modify<F>(&self, f: F)
            where
                for<'w> F: FnOnce(&R, &'w mut W) -> &'w mut W
            {
                let bits = self.register.get();
                let r = R { bits: bits };
                let mut w = W { bits: bits };
                f(&r, &mut w);
                self.register.set(w.bits);
            }
        });
    }

    if access == Access::ReadOnly || access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Reads the contents of the register
            #[inline(always)]
            pub fn read(&self) -> R {
                R { bits: self.register.get() }
            }
        });

        mod_items.push(quote! {
            /// Value read from the register
            pub struct R {
                bits: #rty,
            }
        });

        r_impl_items.push(quote! {
            /// Value of the register as raw bits
            #[inline(always)]
            pub fn bits(&self) -> #rty {
                self.bits
            }
        });
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Writes to the register
            #[inline(always)]
            pub fn write<F>(&self, f: F)
            where
                F: FnOnce(&mut W) -> &mut W
            {
                let mut w = W::reset_value();
                f(&mut w);
                self.register.set(w.bits);
            }
        });

        mod_items.push(quote! {
            /// Value to write to the register
            pub struct W {
                bits: #rty,
            }
        });

        let rv = register.reset_value
            .or(defs.reset_value)
            .map(|rv| util::unsuffixed(u64(rv)))
            .ok_or_else(|| {
                            format!("Register {} has no reset value",
                                    register.name)
                        })?;

        w_impl_items.push(quote! {
            /// Reset value of the register
            #[inline(always)]
            pub fn reset_value() -> W {
                W { bits: #rv }
            }

            /// Writes raw bits to the register
            #[inline(always)]
            pub unsafe fn bits(&mut self, bits: #rty) -> &mut Self {
                self.bits = bits;
                self
            }
        });
    }

    mod_items.push(quote! {
        impl super::#name_pc {
            #(#reg_impl_items)*
        }
    });

    if let Some(fields) = register.fields.as_ref() {
        if !fields.is_empty() {
            ::generate::fields(fields,
                               register,
                               all_registers,
                               peripheral,
                               &rty,
                               access,
                               &mut mod_items,
                               &mut r_impl_items,
                               &mut w_impl_items)?;
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

    items.push(quote! {
        #[doc = #description]
        pub struct #name_pc {
            register: VolatileCell<#rty>
        }

        #[doc = #description]
        pub mod #name_sc {
            #(#mod_items)*
        }
    });

    Ok(())
}

pub fn fields(
    fields: &[Field],
    parent: &Register,
    all_registers: &[Register],
    peripheral: &Peripheral,
    rty: &Ident,
    access: Access,
    mod_items: &mut Vec<Tokens>,
    r_impl_items: &mut Vec<Tokens>,
    w_impl_items: &mut Vec<Tokens>,
) -> Result<()> {
    struct F<'a> {
        _pc_w: Ident,
        _sc: Ident,
        access: Option<Access>,
        description: String,
        evs: &'a [EnumeratedValues],
        mask: Lit,
        name: &'a str,
        offset: Lit,
        pc_r: Ident,
        pc_w: Ident,
        sc: Ident,
        ty: Ident,
        width: u32,
    }

    impl<'a> F<'a> {
        fn from(f: &'a Field) -> Result<Self> {
            let BitRange { offset, width } = f.bit_range;
            let sc = f.name.to_sanitized_snake_case();
            let pc = f.name.to_sanitized_pascal_case();
            let pc_r = Ident::new(&*format!("{}R", pc));
            let pc_w = Ident::new(&*format!("{}W", pc));
            let _pc_w = Ident::new(&*format!("_{}W", pc));
            let _sc = Ident::new(&*format!("_{}", sc));
            let mut description = if width == 1 {
                format!("Bit {}", offset)
            } else {
                format!("Bits {}:{}", offset, offset + width - 1)
            };
            if let Some(ref d) = f.description {
                description.push_str(" - ");
                description.push_str(&*util::respace(d));
            }
            Ok(F {
                   _pc_w: _pc_w,
                   _sc: _sc,
                   description: description,
                   pc_r: pc_r,
                   pc_w: pc_w,
                   width: width,
                   access: f.access,
                   evs: &f.enumerated_values,
                   sc: Ident::new(&*sc),
                   mask: util::unsuffixed((1 << width) - 1),
                   name: &f.name,
                   offset: util::unsuffixed(u64(f.bit_range.offset)),
                   ty: width.to_ty()?,
               })
        }
    }

    let fs = fields.iter()
        .map(F::from)
        .collect::<Result<Vec<_>>>()?;

    // TODO enumeratedValues
    if access == Access::ReadOnly || access == Access::ReadWrite {
        for f in &fs {
            if f.access == Some(Access::WriteOnly) {
                continue;
            }

            let mask = &f.mask;
            let offset = &f.offset;
            let fty = &f.ty;
            let bits = quote! {
                const MASK: #fty = #mask;
                const OFFSET: u8 = #offset;

                ((self.bits >> OFFSET) & MASK as #rty) as #fty
            };

            if let Some((evs, base)) =
                util::lookup(f.evs,
                             fields,
                             parent,
                             all_registers,
                             peripheral,
                             Usage::Read)? {
                struct Variant<'a> {
                    description: &'a str,
                    pc: Ident,
                    sc: Ident,
                    value: u64,
                }

                let has_reserved_variant = evs.values.len() != (1 << f.width);
                let variants = evs.values
                    .iter()
                    .map(|ev| {
                        let sc =
                            Ident::new(&*ev.name.to_sanitized_snake_case());
                        let description = ev.description
                            .as_ref()
                            .map(|s| &**s)
                            .unwrap_or("undocumented");

                        let value = u64(ev.value.ok_or_else(|| {
                            format!("EnumeratedValue {} has no <value> field",
                                    ev.name)
                        })?);
                        Ok(Variant {
                            description: description,
                            sc: sc,
                            pc: Ident::new(&*ev.name
                                           .to_sanitized_pascal_case()),
                            value: value,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let pc_r = &f.pc_r;
                if let Some(ref base) = base {
                    let pc = base.field.to_sanitized_pascal_case();
                    let base_pc_r = Ident::new(&*format!("{}R", pc));
                    let description = format!("Possible values of \
                                               the field `{}`",
                                              f.name);

                    if let Some(ref register) = base.register {
                        let mod_ =
                            Ident::new(&*register.to_sanitized_snake_case());

                        mod_items.push(quote! {
                            #[doc = #description]
                            pub type #pc_r = super::#mod_::#base_pc_r;
                        });
                    } else {
                        mod_items.push(quote! {
                            #[doc = #description]
                            pub type #pc_r = #base_pc_r;
                        });
                    }
                }

                let description = &f.description;
                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description]
                    #[inline(always)]
                    pub fn #sc(&self) -> #pc_r {
                        #pc_r::_from({ #bits })
                    }
                });

                if base.is_none() {
                    let desc = format!("Possible values of the field `{}`",
                                       f.name);

                    let mut vars = variants.iter()
                        .map(|v| {
                            let desc = v.description;
                            let pc = &v.pc;
                            quote! {
                                #[doc = #desc]
                                #pc
                            }
                        })
                        .collect::<Vec<_>>();
                    if has_reserved_variant {
                        vars.push(quote! {
                            /// Reserved
                            _Reserved(#fty)
                        });
                    }
                    mod_items.push(quote! {
                        #[doc = #desc]
                        #[derive(Clone, Copy, Debug, PartialEq)]
                        pub enum #pc_r {
                            #(#vars),*
                        }
                    });

                    let mut enum_items = vec![];

                    let mut arms = variants.iter()
                        .map(|v| {
                            let value = util::unsuffixed(v.value);
                            let pc = &v.pc;

                            quote! {
                                #pc_r::#pc => #value
                            }
                        })
                        .collect::<Vec<_>>();
                    if has_reserved_variant {
                        arms.push(quote! {
                            #pc_r::_Reserved(bits) => bits
                        });
                    }
                    enum_items.push(quote! {
                        /// Value of the field as raw bits
                        #[inline(always)]
                        pub fn bits(&self) -> #fty {
                            match *self {
                                #(#arms),*
                            }
                        }
                    });

                    let mut arms = variants.iter()
                        .map(|v| {
                            let i = util::unsuffixed(v.value);
                            let pc = &v.pc;

                            quote! {
                                #i => #pc_r::#pc
                            }
                        })
                        .collect::<Vec<_>>();

                    if has_reserved_variant {
                        arms.push(quote! {
                            i => #pc_r::_Reserved(i)
                        });
                    } else {
                        arms.push(quote! {
                            _ => unreachable!()
                        });
                    }

                    enum_items.push(quote! {
                        #[allow(missing_docs)]
                        #[doc(hidden)]
                        #[inline(always)]
                        pub fn _from(bits: #fty) -> #pc_r {
                            match bits {
                                #(#arms),*,
                            }
                        }
                    });

                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.sc;

                        let is_variant = if sc.as_ref().starts_with("_") {
                            Ident::new(&*format!("is{}", sc))
                        } else {
                            Ident::new(&*format!("is_{}", sc))
                        };

                        let doc = format!("Checks if \
                                                   the value of the field \
                                                   is `{}`",
                                          pc);
                        enum_items.push(quote! {
                            #[doc = #doc]
                            #[inline(always)]
                            pub fn #is_variant(&self) -> bool {
                                *self == #pc_r::#pc
                            }
                        });
                    }

                    mod_items.push(quote! {
                        impl #pc_r {
                            #(#enum_items)*
                        }
                    });
                }
            } else {
                let description = &f.description;
                let pc_r = &f.pc_r;
                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description]
                    #[inline(always)]
                    pub fn #sc(&self) -> #pc_r {
                        let bits = { # bits };
                        #pc_r { bits }
                    }
                });

                mod_items.push(quote! {
                    /// Value of the field
                    pub struct #pc_r {
                        bits: #fty,
                    }

                    impl #pc_r {
                        /// Value of the field as raw bits
                        #[inline(always)]
                        pub fn bits(&self) -> #fty {
                            self.bits
                        }
                    }
                });
            }

        }
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {
        for f in &fs {
            if f.access == Some(Access::ReadOnly) {
                continue;
            }

            let mut proxy_items = vec![];

            let mut safety = Some(Ident::new("unsafe"));
            let fty = &f.ty;
            let offset = &f.offset;
            let mask = &f.mask;

            if let Some((evs, base)) =
                util::lookup(&f.evs,
                             fields,
                             parent,
                             all_registers,
                             peripheral,
                             Usage::Write)? {
                struct Variant {
                    doc: String,
                    pc: Ident,
                    sc: Ident,
                    value: u64,
                }

                let pc_w = &f.pc_w;
                let pc_w_doc = format!("Values that can be written to \
                                        the field `{}`",
                                       f.name);

                let base_pc_w = base.as_ref().map(|base| {
                    let pc = base.field.to_sanitized_pascal_case();
                    let base_pc_w = Ident::new(&*format!("{}W", pc));

                    if let Some(ref register) = base.register {
                        let mod_ =
                            Ident::new(&*register.to_sanitized_snake_case());

                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w = super::#mod_::#base_pc_w;
                        });
                    } else {
                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w = #base_pc_w;
                        });
                    }

                    base_pc_w
                });

                let variants = evs.values
                    .iter()
                    .map(|ev| {
                        let value = u64(ev.value.ok_or_else(|| {
                            format!("EnumeratedValue {} has no `<value>` field",
                                    ev.name)})?);

                        Ok(Variant {
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
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                if variants.len() == 1 << f.width {
                    safety = None;
                }

                if base.is_none() {
                    let variants_pc = variants.iter().map(|v| &v.pc);
                    let variants_doc = variants.iter().map(|v| &*v.doc);
                    mod_items.push(quote! {
                        #[doc = #pc_w_doc]
                        pub enum #pc_w {
                            #(#[doc = #variants_doc]
                              #variants_pc),*
                        }
                    });

                    let arms = variants.iter().map(|v| {
                        let pc = &v.pc;
                        let value = util::unsuffixed(v.value);

                        quote! {
                            #pc_w::#pc => #value
                        }
                    });

                    mod_items.push(quote! {
                        impl #pc_w {
                            #[allow(missing_docs)]
                            #[doc(hidden)]
                            #[inline(always)]
                            pub fn _bits(&self) -> #fty {
                                match *self {
                                    #(#arms),*
                                }
                            }
                        }
                    });
                }


                proxy_items.push(quote! {
                    /// Writes `variant` to the field
                    #[inline(always)]
                    pub fn variant(self, variant: #pc_w) -> &'a mut W {
                        #safety {
                            self.bits(variant._bits())
                        }
                    }
                });

                for v in &variants {
                    let pc = &v.pc;
                    let sc = &v.sc;

                    let doc = util::respace(&v.doc);
                    let enum_ = base_pc_w.as_ref().unwrap_or(&pc_w);
                    proxy_items.push(quote! {
                        #[doc = #doc]
                        #[inline(always)]
                        pub fn #sc(self) -> &'a mut W {
                            self.variant(#enum_::#pc)
                        }
                    });
                }
            }

            proxy_items.push(quote! {
                /// Writes raw bits to the field
                #[inline(always)]
                pub #safety fn bits(self, bits: #fty) -> &'a mut W {
                    const MASK: #fty = #mask;
                    const OFFSET: u8 = #offset;

                    self.w.bits &= !((MASK as #rty) << OFFSET);
                    self.w.bits |= ((bits & MASK) as #rty) << OFFSET;
                    self.w
                }
            });

            let _pc_w = &f._pc_w;
            mod_items.push(quote! {
                /// Proxy
                pub struct #_pc_w<'a> {
                    w: &'a mut W,
                }

                impl<'a> #_pc_w<'a> {
                    #(#proxy_items)*
                }
            });

            let description = &f.description;
            let sc = &f.sc;
            w_impl_items.push(quote! {
                #[doc = #description]
                #[inline(always)]
                pub fn #sc(&mut self) -> #_pc_w {
                    #_pc_w { w: self }
                }
            })
        }
    }

    Ok(())
}
