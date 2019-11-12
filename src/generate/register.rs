use crate::svd::{
    Access, BitRange, RegisterProperties, EnumeratedValues, Field, Peripheral, Register, RegisterCluster,
    Usage, WriteConstraint,
};
use cast::u64;
use log::warn;
use proc_macro2::{TokenStream, Ident, Span, Punct, Spacing};

use crate::errors::*;
use crate::util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase, U32Ext};

pub fn render(
    register: &Register,
    all_registers: &[&Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    defs: &RegisterProperties,
) -> Result<Vec<TokenStream>> {
    let access = util::access_of(register);
    let name = util::name_of(register);
    let span = Span::call_site();
    let name_pc = Ident::new(&name.to_sanitized_upper_case(), span);
    let _name_pc = Ident::new(&format!("_{}", &name.to_sanitized_upper_case()), span);
    let name_sc = Ident::new(&name.to_sanitized_snake_case(), span);
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
    let description = util::escape_brackets(
        util::respace(&register.description.clone().unwrap_or_else(|| {
            warn!("Missing description for register {}", register.name);
            "".to_string()
        }))
        .as_ref(),
    );

    let mut mod_items = vec![];
    let mut r_impl_items = vec![];
    let mut w_impl_items = vec![];
    let mut methods = vec![];

    let can_read = [Access::ReadOnly, Access::ReadWriteOnce, Access::ReadWrite].contains(&access);
    let can_write = access != Access::ReadOnly;

    if can_read {
        let desc = format!("Reader of register {}", register.name);
        mod_items.push(quote! {
            #[doc = #desc]
            pub type R = crate::R<#rty, super::#name_pc>;
        });
        methods.push("read");
    }

    let res_val = register
        .reset_value
        .or(defs.reset_value)
        .map(|v| v as u64);
    if can_write {
        let desc = format!("Writer for register {}", register.name);
        mod_items.push(quote! {
            #[doc = #desc]
            pub type W = crate::W<#rty, super::#name_pc>;
        });
        if let Some(rv) = res_val.map(util::hex) {
            let doc = format!("Register {} `reset()`'s with value {}", register.name, &rv);
            mod_items.push(quote! {
                #[doc = #doc]
                impl crate::ResetValue for super::#name_pc {
                    type Type = #rty;
                    #[inline(always)]
                    fn reset_value() -> Self::Type { #rv }
                }
            });
            methods.push("reset");
            methods.push("write");
        }
        methods.push("write_with_zero");
    }

    if can_read && can_write {
        methods.push("modify");
    }

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
                res_val,
                access,
                &mut mod_items,
                &mut r_impl_items,
                &mut w_impl_items,
            )?;
        }
    }

    let open = Punct::new('{', Spacing::Alone);
    let close = Punct::new('}', Spacing::Alone);

    if can_read {
        mod_items.push(quote! {
            impl R #open
        });

        for item in r_impl_items {
            mod_items.push(quote! {
                #item
            });
        }

        mod_items.push(quote! {
            #close
        });
    }

    if can_write {
        mod_items.push(quote! {
            impl W #open
        });

        for item in w_impl_items {
            mod_items.push(quote! {
                #item
            });
        }

        mod_items.push(quote! {
            #close
        });
    }

    let mut out = vec![];
    let methods = methods.iter().map(|s| format!("[`{0}`](crate::generic::Reg::{0})", s)).collect::<Vec<_>>();
    let mut doc = format!("{}\n\nThis register you can {}. See [API](https://docs.rs/svd2rust/#read--modify--write-api).",
                        &description, methods.join(", "));

    if name_sc != "cfg" {
        doc += format!("\n\nFor information about available fields see [{0}]({0}) module", &name_sc).as_str();
    }
    out.push(quote! {
        #[doc = #doc]
        pub type #name_pc = crate::Reg<#rty, #_name_pc>;

        #[allow(missing_docs)]
        #[doc(hidden)]
        pub struct #_name_pc;
    });

    if can_read {
        let doc = format!("`read()` method returns [{0}::R]({0}::R) reader structure", &name_sc);
        out.push(quote! {
            #[doc = #doc]
            impl crate::Readable for #name_pc {}
        });
    }
    if can_write {
        let doc = format!("`write(|w| ..)` method takes [{0}::W]({0}::W) writer structure", &name_sc);
        out.push(quote! {
            #[doc = #doc]
            impl crate::Writable for #name_pc {}
        });
    }

    out.push(quote! {
        #[doc = #description]
        pub mod #name_sc #open
    });

    for item in mod_items {
        out.push(quote! {
            #item
        });
    }

    out.push(quote! {
        #close
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
    reset_value: Option<u64>,
    access: Access,
    mod_items: &mut Vec<TokenStream>,
    r_impl_items: &mut Vec<TokenStream>,
    w_impl_items: &mut Vec<TokenStream>,
) -> Result<()> {
    impl<'a> F<'a> {
        fn from(f: &'a Field) -> Result<Self> {
            // TODO(AJM) - do we need to do anything with this range type?
            let BitRange {
                offset,
                width,
                range_type: _,
            } = f.bit_range;
            let sc = f.name.to_sanitized_snake_case();
            let pc = f.name.to_sanitized_upper_case();
            let span = Span::call_site();
            let pc_r = Ident::new(&format!("{}_A", pc), span);
            let _pc_r = Ident::new(&format!("{}_R", pc), span);
            let pc_w = Ident::new(&format!("{}_AW", pc), span);
            let _pc_w = Ident::new(&format!("{}_W", pc), span);
            let _sc = Ident::new(&format!("_{}", sc), span);
            let bits = Ident::new(if width == 1 {
                "bit"
            } else {
                "bits"
            }, Span::call_site());
            let mut description_with_bits = if width == 1 {
                format!("Bit {}", offset)
            } else {
                format!("Bits {}:{}", offset, offset + width - 1)
            };
            if let Some(d) = &f.description {
                description_with_bits.push_str(" - ");
                description_with_bits.push_str(&util::respace(&util::escape_brackets(d)));
            }
            let description = if let Some(d) = &f.description {
                util::respace(&util::escape_brackets(d))
            } else {
                "".to_owned()
            };

            Ok(F {
                _pc_w,
                _sc,
                description,
                description_with_bits,
                pc_r,
                _pc_r,
                pc_w,
                bits,
                width,
                access: f.access,
                evs: &f.enumerated_values,
                sc: Ident::new(&sc, Span::call_site()),
                mask: 1u64.wrapping_neg() >> (64-width),
                name: &f.name,
                offset: u64::from(offset),
                ty: width.to_ty()?,
                write_constraint: f.write_constraint.as_ref(),
            })
        }
    }

    let fs = fields.iter().map(F::from).collect::<Result<Vec<_>>>()?;

    // TODO enumeratedValues
    for f in &fs {
        let can_read = [Access::ReadOnly, Access::ReadWriteOnce, Access::ReadWrite]
            .contains(&access)
            && (f.access != Some(Access::WriteOnly))
            && (f.access != Some(Access::WriteOnce));
        let can_write = (access != Access::ReadOnly) && (f.access != Some(Access::ReadOnly));

        let bits = &f.bits;
        let mask = &util::hex(f.mask);
        let offset = f.offset;
        let rv = reset_value.map(|rv| (rv >> offset) & f.mask);
        let fty = &f.ty;

        let lookup_results = lookup(
            &f.evs,
            fields,
            parent,
            all_registers,
            peripheral,
            all_peripherals,
        )?;


        let pc_r = &f.pc_r;
        let mut pc_w = &f.pc_r;

        let mut base_pc_w = None;
        let mut evs_r = None;

        let _pc_r = &f._pc_r;
        let _pc_w = &f._pc_w;
        let description = &f.description;
        let description_with_bits = &f.description_with_bits;

        if can_read {
            let cast = if f.width == 1 {
                quote! { != 0 }
            } else {
                quote! { as #fty }
            };
            let value = if offset != 0 {
                let offset = &util::unsuffixed(offset);
                quote! {
                    ((self.bits >> #offset) & #mask) #cast
                }
            } else {
                quote! {
                    (self.bits & #mask) #cast
                }
            };

            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Read) {
                evs_r = Some(evs.clone());

                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description_with_bits]
                    #[inline(always)]
                    pub fn #sc(&self) -> #_pc_r {
                        #_pc_r::new( #value )
                    }
                });

                base_pc_w = base.as_ref().map(|base| {
                    let pc = base.field.to_sanitized_upper_case();
                    let base_pc_r = Ident::new(&format!("{}_A", pc), Span::call_site());
                    let base_pc_r = derive_from_base(mod_items, &base, &pc_r, &base_pc_r, description);

                    let doc = format!("Reader of field `{}`", f.name);
                    mod_items.push(quote! {
                        #[doc = #doc]
                        pub type #_pc_r = crate::R<#fty, #base_pc_r>;
                    });

                    base_pc_r
                });

                if base.is_none() {
                    let has_reserved_variant = evs.values.len() != (1 << f.width);
                    let variants = Variant::from_enumerated_values(evs)?;

                    add_from_variants(mod_items, &variants, pc_r, &f, description, rv);

                    let mut enum_items = vec![];

                    let mut arms = variants
                        .iter()
                        .map(|v| {
                            let i = util::unsuffixed_or_bool(v.value, f.width);
                            let pc = &v.pc;

                            if has_reserved_variant {
                                quote! { #i => Val(#pc_r::#pc) }
                            } else {
                                quote! { #i => #pc_r::#pc }
                            }
                        })
                        .collect::<Vec<_>>();

                    if has_reserved_variant {
                        arms.push(quote! {
                            i => Res(i)
                        });
                    } else if 1 << f.width.to_ty_width()? != variants.len() {
                        arms.push(quote! {
                            _ => unreachable!()
                        });
                    }

                    if has_reserved_variant {
                        enum_items.push(quote! {
                            ///Get enumerated values variant
                            #[inline(always)]
                            pub fn variant(&self) -> crate::Variant<#fty, #pc_r> {
                                use crate::Variant::*;
                                match self.bits {
                                    #(#arms),*
                                }
                            }
                        });
                    } else {
                        enum_items.push(quote! {
                            ///Get enumerated values variant
                            #[inline(always)]
                            pub fn variant(&self) -> #pc_r {
                                match self.bits {
                                    #(#arms),*
                                }
                            }
                        });
                    }

                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.sc;

                        let is_variant = Ident::new(&if sc.to_string().starts_with('_') {
                            format!("is{}", sc)
                        } else {
                            format!("is_{}", sc)
                        }, Span::call_site());

                        let doc = format!("Checks if the value of the field is `{}`", pc);
                        enum_items.push(quote! {
                            #[doc = #doc]
                            #[inline(always)]
                            pub fn #is_variant(&self) -> bool {
                                *self == #pc_r::#pc
                            }
                        });
                    }

                    let doc = format!("Reader of field `{}`", f.name);
                    mod_items.push(quote! {
                        #[doc = #doc]
                        pub type #_pc_r = crate::R<#fty, #pc_r>;
                        impl #_pc_r {
                            #(#enum_items)*
                        }
                    });
                }

            } else {
                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description_with_bits]
                    #[inline(always)]
                    pub fn #sc(&self) -> #_pc_r {
                        #_pc_r::new ( #value )
                    }
                });

                let doc = format!("Reader of field `{}`", f.name);
                mod_items.push(quote! {
                    #[doc = #doc]
                    pub type #_pc_r = crate::R<#fty, #fty>;
                })

            }
        }

        if can_write {
            let mut proxy_items = vec![];

            let mut unsafety = unsafety(f.write_constraint, f.width);
            let width = f.width;

            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Write) {
                let variants = Variant::from_enumerated_values(evs)?;

                if variants.len() == 1 << f.width {
                    unsafety = None;
                }

                if Some(evs) != evs_r.as_ref() {
                    pc_w = &f.pc_w;

                    base_pc_w = base.as_ref().map(|base| {
                        let pc = base.field.to_sanitized_upper_case();
                        let base_pc_w = Ident::new(&format!("{}_AW", pc), Span::call_site());
                        derive_from_base(mod_items, &base, &pc_w, &base_pc_w, description)
                    });

                    if base.is_none() {
                        add_from_variants(mod_items, &variants, pc_w, &f, description, rv);
                    }
                }

                proxy_items.push(quote! {
                    ///Writes `variant` to the field
                    #[inline(always)]
                    pub fn variant(self, variant: #pc_w) -> &'a mut W {
                        #unsafety {
                            self.#bits(variant.into())
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
                            #[inline(always)]
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#enum_::#pc)
                            }
                        });
                    } else {
                        proxy_items.push(quote! {
                            #[doc = #doc]
                            #[inline(always)]
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#pc_w::#pc)
                            }
                        });
                    }
                }
            }

            if width == 1 {
                proxy_items.push(quote! {
                    ///Sets the field bit
                    #[inline(always)]
                    pub #unsafety fn set_bit(self) -> &'a mut W {
                        self.bit(true)
                    }

                    ///Clears the field bit
                    #[inline(always)]
                    pub #unsafety fn clear_bit(self) -> &'a mut W {
                        self.bit(false)
                    }
                });
            }

            proxy_items.push(if offset != 0 {
                let offset = &util::unsuffixed(offset);
                quote! {
                    ///Writes raw bits to the field
                    #[inline(always)]
                    pub #unsafety fn #bits(self, value: #fty) -> &'a mut W {
                        self.w.bits = (self.w.bits & !(#mask << #offset)) | (((value as #rty) & #mask) << #offset);
                        self.w
                    }
                }
            } else {
                quote! {
                    ///Writes raw bits to the field
                    #[inline(always)]
                    pub #unsafety fn #bits(self, value: #fty) -> &'a mut W {
                        self.w.bits = (self.w.bits & !#mask) | ((value as #rty) & #mask);
                        self.w
                    }
                }
            });

            let doc = format!("Write proxy for field `{}`", f.name);
            mod_items.push(quote! {
                #[doc = #doc]
                pub struct #_pc_w<'a> {
                    w: &'a mut W,
                }

                impl<'a> #_pc_w<'a> {
                    #(#proxy_items)*
                }
            });

            let sc = &f.sc;
            w_impl_items.push(quote! {
                #[doc = #description_with_bits]
                #[inline(always)]
                pub fn #sc(&mut self) -> #_pc_w {
                    #_pc_w { w: self }
                }
            })
        }
    }

    Ok(())
}

fn unsafety(write_constraint: Option<&WriteConstraint>, width: u32) -> Option<Ident> {
    match &write_constraint {
        Some(&WriteConstraint::Range(range))
            if u64::from(range.min) == 0 && u64::from(range.max) == 1u64.wrapping_neg() >> (64-width) =>
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
        _ => Some(Ident::new("unsafe", Span::call_site())),
    }
}

struct Variant {
    doc: String,
    pc: Ident,
    sc: Ident,
    value: u64,
}

impl Variant {
    fn from_enumerated_values(evs: &EnumeratedValues) -> Result<Vec<Self>> {
        let span = Span::call_site();
        evs.values
            .iter()
            // filter out all reserved variants, as we should not
            // generate code for them
            .filter(|field| field.name.to_lowercase() != "reserved")
            .map(|ev| {
                let value = u64(ev.value.ok_or_else(|| {
                    format!("EnumeratedValue {} has no `<value>` field", ev.name)
                })?);

                Ok(Variant {
                    doc: ev
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("`{:b}`", value)),
                    pc: Ident::new(&ev.name.to_sanitized_upper_case(), span),
                    sc: Ident::new(&ev.name.to_sanitized_snake_case(), span),
                    value,
                })
            })
            .collect::<Result<Vec<_>>>()
    }
}

fn add_from_variants(mod_items: &mut Vec<TokenStream>, variants: &Vec<Variant>, pc: &Ident, f: &F, desc: &str, reset_value: Option<u64>) {
    let fty = &f.ty;

    let vars = variants
        .iter()
        .map(|v| {
            let desc = util::escape_brackets(&format!("{}: {}", v.value, v.doc));
            let pcv = &v.pc;
            quote! {
                #[doc = #desc]
                #pcv
            }
        })
        .collect::<Vec<_>>();

    let desc = if let Some(rv) = reset_value {
        format!("{}\n\nValue on reset: {}", desc, rv)
    } else {
        desc.to_owned()
    };

    mod_items.push(quote! {
        #[doc = #desc]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum #pc {
            #(#vars),*
        }
    });

    let arms = variants.iter().map(|v| {
        let pcv = &v.pc;
        let value = util::unsuffixed_or_bool(v.value, f.width);

        quote! {
            #pc::#pcv => #value
        }
    });

    mod_items.push(quote! {
        impl From<#pc> for #fty {
            #[inline(always)]
            fn from(variant: #pc) -> Self {
                match variant {
                    #(#arms),*
                }
            }
        }
    });
}

fn derive_from_base(mod_items: &mut Vec<TokenStream>, base: &Base, pc: &Ident, base_pc: &Ident, desc: &str) -> TokenStream {
    let span = Span::call_site();
    if let (Some(peripheral), Some(register)) = (&base.peripheral, &base.register) {
        let pmod_ = peripheral.to_sanitized_snake_case();
        let rmod_ = register.to_sanitized_snake_case();
        let pmod_ = Ident::new(&pmod_, span);
        let rmod_ = Ident::new(&rmod_, span);

        mod_items.push(quote! {
            #[doc = #desc]
            pub type #pc =
                crate::#pmod_::#rmod_::#base_pc;
        });

        quote! {
            crate::#pmod_::#rmod_::#base_pc
        }
    } else if let Some(register) = &base.register {
        let mod_ = register.to_sanitized_snake_case();
        let mod_ = Ident::new(&mod_, span);

        mod_items.push(quote! {
            #[doc = #desc]
            pub type #pc =
                super::#mod_::#base_pc;
        });

        quote! {
            super::#mod_::#base_pc
        }
    } else {
        mod_items.push(quote! {
            #[doc = #desc]
            pub type #pc = #base_pc;
        });

        quote! {
            #base_pc
        }
    }
}

struct F<'a> {
    _pc_w: Ident,
    _sc: Ident,
    access: Option<Access>,
    description: String,
    description_with_bits: String,
    evs: &'a [EnumeratedValues],
    mask: u64,
    name: &'a str,
    offset: u64,
    pc_r: Ident,
    _pc_r: Ident,
    pc_w: Ident,
    sc: Ident,
    bits: Ident,
    ty: Ident,
    width: u32,
    write_constraint: Option<&'a WriteConstraint>,
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
) -> Result<Vec<(&'a EnumeratedValues, Option<Base<'a>>)>> {
    let evs = evs
        .iter()
        .map(|evs| {
            if let Some(base) = &evs.derived_from {
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

    Ok(evs)
}

fn lookup_filter<'a>(
    evs: &Vec<(&'a EnumeratedValues, Option<Base<'a>>)>,
    usage: Usage,
) -> Option<(&'a EnumeratedValues, Option<Base<'a>>)> {
    for (evs, base) in evs.iter() {
        if evs.usage == Some(usage) {
            return Some((*evs, base.clone()));
        }
    }

    evs.first().cloned()
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
        if let Some(evs) = f
            .enumerated_values
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
        Some(&(evs, field)) => {
            if matches.len() == 1 {
                Ok((
                    evs,
                    Some(Base {
                        field,
                        register: None,
                        peripheral: None,
                    }),
                ))
            } else {
                let fields = matches.iter().map(|(f, _)| &f.name).collect::<Vec<_>>();
                Err(format!(
                    "Fields {:?} have an \
                     enumeratedValues named {}",
                    fields, base_evs
                ))?
            }
        }
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
    let mut rem: Vec<&RegisterCluster> = Vec::new();
    if p.registers.is_none() {
        return par;
    }

    if let Some(regs) = &p.registers {
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
        match b {
            RegisterCluster::Register(reg) => {
                par.push(reg);
            }
            RegisterCluster::Cluster(cluster) => {
                for c in cluster.children.iter() {
                    rem.push(c);
                }
            }
        }
    }
    par
}
