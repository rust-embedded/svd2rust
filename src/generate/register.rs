use cast::u64;
use either::Either;
use quote::Tokens;
use svd::{Access, BitRange, Cluster, Defaults, EnumeratedValues, Field, Peripheral, Register,
          Usage, WriteConstraint};
use syn::Ident;

use errors::*;
use util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase, U32Ext};

pub fn render(
    register: &Register,
    all_registers: &[&Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    defs: &Defaults,
) -> Result<Vec<Tokens>> {
    let access = util::access_of(register);
    let name = util::name_of(register);
    let name_pc = Ident::new(&*name.to_sanitized_upper_case());
    let name_sc = Ident::new(&*name.to_sanitized_snake_case());
    let rsize = register
        .size
        .or(defs.size)
        .ok_or_else(|| format!("Register {} has no `size` field", register.name))?;
    let rsize = if rsize < 8 {
        8
    } else if rsize.is_power_of_two() {
        rsize
    } else {
        rsize.next_power_of_two()
    };
    let rty = rsize.to_ty()?;
    let description = util::escape_brackets(util::respace(&register.description).as_ref());

    let unsafety = unsafety(register.write_constraint.as_ref(), rsize);

    let mut mod_items = vec![];
    let mut reg_impl_items = vec![];
    let mut r_impl_items = vec![];
    let mut w_impl_items = vec![];

    if access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Modifies the contents of the register
            #[inline]
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
            #[inline]
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
            #[inline]
            pub fn bits(&self) -> #rty {
                self.bits
            }
        });
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {


        mod_items.push(quote! {
            /// Value to write to the register
            pub struct W {
                bits: #rty,
            }
        });

        let rv = register
            .reset_value
            .or(defs.reset_value)
            .map(util::hex);
            //.ok_or_else(|| format!("Register {} has no reset value", register.name))?;
        if let Some(rv) = rv {
            reg_impl_items.push(quote! {
                /// Writes to the register
                #[inline]
                pub fn write<F>(&self, f: F)
                where
                    F: FnOnce(&mut W) -> &mut W
                {
                    let mut w = W::reset_value();
                    f(&mut w);
                    self.register.set(w.bits);
                }
            });
            w_impl_items.push(quote! {
                /// Reset value of the register
                #[inline]
                pub fn reset_value() -> W {
                    W { bits: #rv }
                }

                /// Writes raw bits to the register
                #[inline]
                pub #unsafety fn bits(&mut self, bits: #rty) -> &mut Self {
                    self.bits = bits;
                    self
                }
            });
        }
        else{
            reg_impl_items.push(quote! {
                /// Writes to the register
                #[inline]
                pub fn write<F>(&self, f: F)
                where
                    F: FnOnce(&mut W) -> &mut W
                {
                let bits = self.register.get();
                let mut w = W { bits: bits };
                f(&mut w);
                self.register.set(w.bits);
                }
            });

        }
    }

    if access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Writes the reset value to the register
            #[inline]
            pub fn reset(&self) {
                self.write(|w| w)
            }
        })
    }

    mod_items.push(quote! {
        impl super::#name_pc {
            #(#reg_impl_items)*
        }
    });

    if let Some(cur_fields) = register.fields.as_ref() {
        // filter out all reserved fields, as we should not generate code for
        // them
        let cur_fields: Vec<Field> = cur_fields
            .clone()
            .into_iter()
            .filter(|field| field.name.to_lowercase() != "reserved")
            .collect();

        if !cur_fields.is_empty() {
            fields(
                &cur_fields,
                register,
                all_registers,
                peripheral,
                all_peripherals,
                &rty,
                access,
                &mut mod_items,
                &mut r_impl_items,
                &mut w_impl_items,
            )?;
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

    let mut out = vec![];
    out.push(quote! {
        #[doc = #description]
        pub struct #name_pc {
            register: ::vcell::VolatileCell<#rty>
        }

        #[doc = #description]
        pub mod #name_sc {
            #(#mod_items)*
        }
    });

    Ok(out)
}

pub fn fields(
    fields: &[Field],
    parent: &Register,
    all_registers: &[&Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
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
        mask: Tokens,
        name: &'a str,
        offset: Tokens,
        pc_r: Ident,
        pc_w: Ident,
        sc: Ident,
        bits: Ident,
        ty: Ident,
        width: u32,
        write_constraint: Option<&'a WriteConstraint>,
    }

    impl<'a> F<'a> {
        fn from(f: &'a Field) -> Result<Self> {
            let BitRange { offset, width } = f.bit_range;
            let sc = f.name.to_sanitized_snake_case();
            let pc = f.name.to_sanitized_upper_case();
            let pc_r = Ident::new(&*format!("{}R", pc));
            let pc_w = Ident::new(&*format!("{}W", pc));
            let _pc_w = Ident::new(&*format!("_{}W", pc));
            let _sc = Ident::new(&*format!("_{}", sc));
            let bits = if width == 1 {
                Ident::new("bit")
            } else {
                Ident::new("bits")
            };
            let mut description = if width == 1 {
                format!("Bit {}", offset)
            } else {
                format!("Bits {}:{}", offset, offset + width - 1)
            };
            if let Some(ref d) = f.description {
                description.push_str(" - ");
                description.push_str(&*util::respace(&util::escape_brackets(d)));
            }
            Ok(F {
                _pc_w,
                _sc,
                description,
                pc_r,
                pc_w,
                bits,
                width,
                access: f.access,
                evs: &f.enumerated_values,
                sc: Ident::new(&*sc),
                mask: util::hex_or_bool((((1 as u64) << width) - 1) as u32, width),
                name: &f.name,
                offset: util::unsuffixed(u64::from(f.bit_range.offset)),
                ty: width.to_ty()?,
                write_constraint: f.write_constraint.as_ref(),
            })
        }
    }

    let fs = fields.iter().map(F::from).collect::<Result<Vec<_>>>()?;

    // TODO enumeratedValues
    if access == Access::ReadOnly || access == Access::ReadWrite {
        for f in &fs {
            if f.access == Some(Access::WriteOnly) {
                continue;
            }

            let bits = &f.bits;
            let mask = &f.mask;
            let offset = &f.offset;
            let fty = &f.ty;
            let cast = if f.width == 1 {
                quote! { != 0 }
            } else {
                quote! { as #fty }
            };
            let value = quote! {
                const MASK: #fty = #mask;
                const OFFSET: u8 = #offset;

                ((self.bits >> OFFSET) & MASK as #rty) #cast
            };

            if let Some((evs, base)) = lookup(
                f.evs,
                fields,
                parent,
                all_registers,
                peripheral,
                all_peripherals,
                Usage::Read,
            )? {
                struct Variant<'a> {
                    description: &'a str,
                    pc: Ident,
                    sc: Ident,
                    value: u64,
                }

                let has_reserved_variant = evs.values.len() != (1 << f.width);
                let variants = evs.values
                    .iter()
                    // filter out all reserved variants, as we should not
                    // generate code for them
                    .filter(|field| field.name.to_lowercase() != "reserved")
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
                            description,
                            sc,
                            pc: Ident::new(&*ev.name
                                           .to_sanitized_upper_case()),
                            value,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let pc_r = &f.pc_r;
                if let Some(ref base) = base {
                    let pc = base.field.to_sanitized_upper_case();
                    let base_pc_r = Ident::new(&*format!("{}R", pc));
                    let desc = format!("Possible values of the field `{}`", f.name,);

                    if let (Some(ref peripheral), Some(ref register)) =
                        (base.peripheral, base.register)
                    {
                        let pmod_ = peripheral.to_sanitized_snake_case();
                        let rmod_ = register.to_sanitized_snake_case();
                        let pmod_ = Ident::new(&*pmod_);
                        let rmod_ = Ident::new(&*rmod_);

                        mod_items.push(quote! {
                            #[doc = #desc]
                            pub type #pc_r = ::#pmod_::#rmod_::#base_pc_r;
                        });
                    } else if let Some(ref register) = base.register {
                        let mod_ = register.to_sanitized_snake_case();
                        let mod_ = Ident::new(&*mod_);

                        mod_items.push(quote! {
                            #[doc = #desc]
                            pub type #pc_r = super::#mod_::#base_pc_r;
                        });
                    } else {
                        mod_items.push(quote! {
                            #[doc = #desc]
                            pub type #pc_r = #base_pc_r;
                        });
                    }
                }

                let description = &util::escape_brackets(&f.description);
                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description]
                    #[inline]
                    pub fn #sc(&self) -> #pc_r {
                        #pc_r::_from({ #value })
                    }
                });

                if base.is_none() {
                    let desc = format!("Possible values of the field `{}`", f.name,);

                    let mut vars = variants
                        .iter()
                        .map(|v| {
                            let desc = util::escape_brackets(&v.description);
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

                    let mut arms = variants
                        .iter()
                        .map(|v| {
                            let value = util::hex_or_bool(v.value as u32, f.width);
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

                    if f.width == 1 {
                        enum_items.push(quote! {
                            /// Returns `true` if the bit is clear (0)
                            #[inline]
                            pub fn bit_is_clear(&self) -> bool {
                                !self.#bits()
                            }

                            /// Returns `true` if the bit is set (1)
                            #[inline]
                            pub fn bit_is_set(&self) -> bool {
                                self.#bits()
                            }
                        });
                    }

                    enum_items.push(quote! {
                        /// Value of the field as raw bits
                        #[inline]
                        pub fn #bits(&self) -> #fty {
                            match *self {
                                #(#arms),*
                            }
                        }
                    });

                    let mut arms = variants
                        .iter()
                        .map(|v| {
                            let i = util::unsuffixed_or_bool(v.value, f.width);
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
                    } else if 1 << f.width.to_ty_width()? != variants.len() {
                        arms.push(quote! {
                            _ => unreachable!()
                        });
                    }

                    enum_items.push(quote! {
                        #[allow(missing_docs)]
                        #[doc(hidden)]
                        #[inline]
                        pub fn _from(value: #fty) -> #pc_r {
                            match value {
                                #(#arms),*,
                            }
                        }
                    });

                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.sc;

                        let is_variant = if sc.as_ref().starts_with('_') {
                            Ident::new(&*format!("is{}", sc))
                        } else {
                            Ident::new(&*format!("is_{}", sc))
                        };

                        let doc = format!("Checks if the value of the field is `{}`", pc);
                        enum_items.push(quote! {
                            #[doc = #doc]
                            #[inline]
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
                let description = &util::escape_brackets(&f.description);
                let pc_r = &f.pc_r;
                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description]
                    #[inline]
                    pub fn #sc(&self) -> #pc_r {
                        let bits = { #value };
                        #pc_r { bits }
                    }
                });

                let mut pc_r_impl_items = vec![
                    quote! {
                        /// Value of the field as raw bits
                        #[inline]
                        pub fn #bits(&self) -> #fty {
                            self.bits
                        }
                    },
                ];

                if f.width == 1 {
                    pc_r_impl_items.push(quote! {
                        /// Returns `true` if the bit is clear (0)
                        #[inline]
                        pub fn bit_is_clear(&self) -> bool {
                            !self.#bits()
                        }

                        /// Returns `true` if the bit is set (1)
                        #[inline]
                        pub fn bit_is_set(&self) -> bool {
                            self.#bits()
                        }
                    });
                }

                mod_items.push(quote! {
                    /// Value of the field
                    pub struct #pc_r {
                        bits: #fty,
                    }

                    impl #pc_r {
                        #(#pc_r_impl_items)*
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

            let mut unsafety = unsafety(f.write_constraint, f.width);
            let bits = &f.bits;
            let fty = &f.ty;
            let offset = &f.offset;
            let mask = &f.mask;
            let width = f.width;

            if let Some((evs, base)) = lookup(
                &f.evs,
                fields,
                parent,
                all_registers,
                peripheral,
                all_peripherals,
                Usage::Write,
            )? {
                struct Variant {
                    doc: String,
                    pc: Ident,
                    sc: Ident,
                    value: u64,
                }

                let pc_w = &f.pc_w;
                let pc_w_doc = format!("Values that can be written to the field `{}`", f.name);

                let base_pc_w = base.as_ref().map(|base| {
                    let pc = base.field.to_sanitized_upper_case();
                    let base_pc_w = Ident::new(&*format!("{}W", pc));

                    if let (Some(ref peripheral), Some(ref register)) =
                        (base.peripheral, base.register)
                    {
                        let pmod_ = peripheral.to_sanitized_snake_case();
                        let rmod_ = register.to_sanitized_snake_case();
                        let pmod_ = Ident::new(&*pmod_);
                        let rmod_ = Ident::new(&*rmod_);

                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w =
                                ::#pmod_::#rmod_::#base_pc_w;
                        });

                        quote! {
                            ::#pmod_::#rmod_::#base_pc_w
                        }
                    } else if let Some(ref register) = base.register {
                        let mod_ = register.to_sanitized_snake_case();
                        let mod_ = Ident::new(&*mod_);

                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w =
                                super::#mod_::#base_pc_w;
                        });

                        quote! {
                            super::#mod_::#base_pc_w
                        }
                    } else {
                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w = #base_pc_w;
                        });

                        quote! {
                            #base_pc_w
                        }
                    }
                });

                let variants = evs.values
                    .iter()
                    // filter out all reserved variants, as we should not
                    // generate code for them
                    .filter(|field| field.name.to_lowercase() != "reserved")
                    .map(
                        |ev| {
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
                                           .to_sanitized_upper_case()),
                            sc: Ident::new(&*ev.name
                                           .to_sanitized_snake_case()),
                            value,
                        })
                        },
                    )
                    .collect::<Result<Vec<_>>>()?;

                if variants.len() == 1 << f.width {
                    unsafety = None;
                }

                if base.is_none() {
                    let variants_pc = variants.iter().map(|v| &v.pc);
                    let variants_doc = variants.iter().map(|v| util::escape_brackets(&v.doc).to_owned());
                    mod_items.push(quote! {
                        #[doc = #pc_w_doc]
                        pub enum #pc_w {
                            #(#[doc = #variants_doc]
                            #variants_pc),*
                        }
                    });

                    let arms = variants.iter().map(|v| {
                        let pc = &v.pc;
                        let value = util::unsuffixed_or_bool(v.value, f.width);

                        quote! {
                            #pc_w::#pc => #value
                        }
                    });

                    mod_items.push(quote! {
                        impl #pc_w {
                            #[allow(missing_docs)]
                            #[doc(hidden)]
                            #[inline]
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
                    #[inline]
                    pub fn variant(self, variant: #pc_w) -> &'a mut W {
                        #unsafety {
                            self.#bits(variant._bits())
                        }
                    }
                });

                for v in &variants {
                    let pc = &v.pc;
                    let sc = &v.sc;

                    let doc = util::escape_brackets(util::respace(&v.doc).as_ref());
                    if let Some(enum_) = base_pc_w.as_ref() {
                        proxy_items.push(quote! {
                            #[doc = #doc]
                            #[inline]
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#enum_::#pc)
                            }
                        });
                    } else {
                        proxy_items.push(quote! {
                            #[doc = #doc]
                            #[inline]
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#pc_w::#pc)
                            }
                        });
                    }
                }
            }

            if width == 1 {
                proxy_items.push(quote! {
                    /// Sets the field bit
                    pub #unsafety fn set_bit(self) -> &'a mut W {
                        self.bit(true)
                    }

                    /// Clears the field bit
                    pub #unsafety fn clear_bit(self) -> &'a mut W {
                        self.bit(false)
                    }
                });
            }

            proxy_items.push(quote! {
                /// Writes raw bits to the field
                #[inline]
                pub #unsafety fn #bits(self, value: #fty) -> &'a mut W {
                    const MASK: #fty = #mask;
                    const OFFSET: u8 = #offset;

                    self.w.bits &= !((MASK as #rty) << OFFSET);
                    self.w.bits |= ((value & MASK) as #rty) << OFFSET;
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

            let description = &util::escape_brackets(&f.description);
            let sc = &f.sc;
            w_impl_items.push(quote! {
                #[doc = #description]
                #[inline]
                pub fn #sc(&mut self) -> #_pc_w {
                    #_pc_w { w: self }
                }
            })
        }
    }

    Ok(())
}

fn unsafety(write_constraint: Option<&WriteConstraint>, width: u32) -> Option<Ident> {
    match write_constraint {
        Some(&WriteConstraint::Range(ref range))
            if range.min as u64 == 0 && range.max as u64 == (1u64 << width) - 1 =>
        {
            // the SVD has acknowledged that it's safe to write
            // any value that can fit in the field
            None
        }
        None if width == 1 => {
            // the field is one bit wide, so we assume it's legal to write
            // either value into it or it wouldn't exist; despite that
            // if a writeConstraint exists then respect it
            None
        }
        _ => Some(Ident::new("unsafe")),
    }
}

#[derive(Clone, Debug)]
pub struct Base<'a> {
    pub peripheral: Option<&'a str>,
    pub register: Option<&'a str>,
    pub field: &'a str,
}

fn lookup<'a>(
    evs: &'a [EnumeratedValues],
    fields: &'a [Field],
    register: &'a Register,
    all_registers: &'a [&'a Register],
    peripheral: &'a Peripheral,
    all_peripherals: &'a [Peripheral],
    usage: Usage,
) -> Result<Option<(&'a EnumeratedValues, Option<Base<'a>>)>> {
    let evs = evs.iter()
        .map(|evs| {
            if let Some(ref base) = evs.derived_from {
                let mut parts = base.split('.');

                match (parts.next(), parts.next(), parts.next(), parts.next()) {
                    (
                        Some(base_peripheral),
                        Some(base_register),
                        Some(base_field),
                        Some(base_evs),
                    ) => lookup_in_peripherals(
                        base_peripheral,
                        base_register,
                        base_field,
                        base_evs,
                        all_peripherals,
                    ),
                    (Some(base_register), Some(base_field), Some(base_evs), None) => {
                        lookup_in_peripheral(
                            None,
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
                    (Some(base_evs), None, None, None) => lookup_in_register(base_evs, register),
                    _ => unreachable!(),
                }
            } else {
                Ok((evs, None))
            }
        })
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
        return lookup_in_field(base_evs, None, None, base_field);
    } else {
        Err(format!(
            "Field {} not found in register {}",
            base_field, register.name
        ))?
    }
}

fn lookup_in_peripheral<'p>(
    base_peripheral: Option<&'p str>,
    base_register: &'p str,
    base_field: &str,
    base_evs: &str,
    all_registers: &[&'p Register],
    peripheral: &'p Peripheral,
) -> Result<(&'p EnumeratedValues, Option<Base<'p>>)> {
    if let Some(register) = all_registers.iter().find(|r| r.name == base_register) {
        if let Some(field) = register
            .fields
            .as_ref()
            .map(|fs| &**fs)
            .unwrap_or(&[])
            .iter()
            .find(|f| f.name == base_field)
        {
            lookup_in_field(base_evs, Some(base_register), base_peripheral, field)
        } else {
            Err(format!(
                "No field {} in register {}",
                base_field, register.name
            ))?
        }
    } else {
        Err(format!(
            "No register {} in peripheral {}",
            base_register, peripheral.name
        ))?
    }
}

fn lookup_in_field<'f>(
    base_evs: &str,
    base_register: Option<&'f str>,
    base_peripheral: Option<&'f str>,
    field: &'f Field,
) -> Result<(&'f EnumeratedValues, Option<Base<'f>>)> {
    for evs in &field.enumerated_values {
        if evs.name.as_ref().map(|s| &**s) == Some(base_evs) {
            return Ok((
                evs,
                Some(Base {
                    field: &field.name,
                    register: base_register,
                    peripheral: base_peripheral,
                }),
            ));
        }
    }

    Err(format!(
        "No EnumeratedValues {} in field {}",
        base_evs, field.name
    ))?
}

fn lookup_in_register<'r>(
    base_evs: &str,
    register: &'r Register,
) -> Result<(&'r EnumeratedValues, Option<Base<'r>>)> {
    let mut matches = vec![];

    for f in register.fields.as_ref().map(|v| &**v).unwrap_or(&[]) {
        if let Some(evs) = f.enumerated_values
            .iter()
            .find(|evs| evs.name.as_ref().map(|s| &**s) == Some(base_evs))
        {
            matches.push((evs, &f.name))
        }
    }

    match matches.first() {
        None => Err(format!(
            "EnumeratedValues {} not found in register {}",
            base_evs, register.name
        ))?,
        Some(&(evs, field)) => if matches.len() == 1 {
            Ok((
                evs,
                Some(Base {
                    field,
                    register: None,
                    peripheral: None,
                }),
            ))
        } else {
            let fields = matches
                .iter()
                .map(|&(ref f, _)| &f.name)
                .collect::<Vec<_>>();
            Err(format!(
                "Fields {:?} have an \
                 enumeratedValues named {}",
                fields, base_evs
            ))?
        },
    }
}

fn lookup_in_peripherals<'p>(
    base_peripheral: &'p str,
    base_register: &'p str,
    base_field: &str,
    base_evs: &str,
    all_peripherals: &'p [Peripheral],
) -> Result<(&'p EnumeratedValues, Option<Base<'p>>)> {
    if let Some(peripheral) = all_peripherals.iter().find(|p| p.name == base_peripheral) {
        let all_registers = periph_all_registers(peripheral);
        lookup_in_peripheral(
            Some(base_peripheral),
            base_register,
            base_field,
            base_evs,
            all_registers.as_slice(),
            peripheral,
        )
    } else {
        Err(format!("No peripheral {}", base_peripheral))?
    }
}

fn periph_all_registers<'a>(p: &'a Peripheral) -> Vec<&'a Register> {
    let mut par: Vec<&Register> = Vec::new();
    let mut rem: Vec<&Either<Register, Cluster>> = Vec::new();
    if p.registers.is_none() {
        return par;
    }

    if let Some(ref regs) = p.registers {
        for r in regs.iter() {
            rem.push(r);
        }
    }

    loop {
        let b = rem.pop();
        if b.is_none() {
            break;
        }

        let b = b.unwrap();
        match *b {
            Either::Left(ref reg) => {
                par.push(reg);
            }
            Either::Right(ref cluster) => for c in cluster.children.iter() {
                rem.push(c);
            },
        }
    }
    par
}
