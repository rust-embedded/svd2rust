use cast::u64;
use quote::Tokens;
use crate::svd::{Access, BitRange, Defaults, EnumeratedValues, Field, Peripheral, Register,
          RegisterCluster, Usage, WriteConstraint};
use syn::Ident;

use crate::errors::*;
use crate::util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase, U32Ext};

pub fn render(
    register: &Register,
    all_registers: &[&Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    defs: &Defaults,
) -> Result<Vec<Tokens>> {
    let access = util::access_of(register);
    let name = util::name_of(register);
    let name_pc = Ident::from(&*name.to_sanitized_upper_case());
    let _name_pc = Ident::from(format!("_{}", &*name.to_sanitized_upper_case()));
    let name_sc = Ident::from(&*name.to_sanitized_snake_case());
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
    let description = util::escape_brackets(util::respace(&register.description.clone().unwrap()).as_ref());

    let mut mod_items = vec![];
    let mut r_impl_items = vec![];
    let mut w_impl_items = vec![];

    let can_read = [Access::ReadOnly, Access::ReadWriteOnce, Access::ReadWrite].contains(&access);
    let can_write = access != Access::ReadOnly;

    if can_read {
        let desc = format!("Reader for register {}", register.name);
        mod_items.push(quote! {
            #[doc = #desc]
            pub type _R = crate::R<#rty, super::#_name_pc>;
        });
    }

    if can_write {
        let rv = register
            .reset_value
            .or(defs.reset_value)
            .map(util::hex)
            .ok_or_else(|| format!("Register {} has no reset value", register.name))?;

        let desc = format!("Writer for register {}", register.name);
        mod_items.push(quote! {
            #[doc = #desc]
            pub type _W = crate::W<#rty, super::#_name_pc>;
            impl crate::ResetValue<#rty> for super::#name_pc {
                #[inline(always)]
                fn reset_value() -> #rty { #rv }
            }
        });
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
                &_name_pc,
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

    if can_read {
        mod_items.push(quote! {
            impl _R {
                #(#r_impl_items)*
            }
        });
    }

    if can_write {
        mod_items.push(quote! {
            impl _W {
                #(#w_impl_items)*
            }
        });
    }

    let mut out = vec![];
    out.push(quote! {
        #[doc = #description]
        pub type #name_pc = crate::Reg<#_name_pc>;

        #[allow(missing_docs)]
        pub struct #_name_pc {
            register: vcell::VolatileCell<#rty>
        }
    });

    if can_read {
        out.push(quote! {
            impl crate::Readable for #_name_pc {}
        });
    }
    if can_write {
        out.push(quote! {
            impl crate::Writable for #_name_pc {}
        });
    }

    out.push(quote! {
        impl core::ops::Deref for #_name_pc {
            type Target = vcell::VolatileCell<#rty>;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                &self.register
            }
        }

        #[doc = #description]
        pub mod #name_sc {
            #(#mod_items)*
        }
    });

    Ok(out)
}

pub fn fields(
    name_pc: &Ident,
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
        pc: Ident,
        pc_r: Ident,
        pcw: Ident,
        sc: Ident,
        ty: Ident,
        width: u32,
        write_constraint: Option<&'a WriteConstraint>,
    }

    impl<'a> F<'a> {
        fn from(f: &'a Field) -> Result<Self> {
            // TODO(AJM) - do we need to do anything with this range type?
            let BitRange { offset, width, range_type: _ } = f.bit_range;
            let sc = f.name.to_sanitized_snake_case();
            let pc_ = f.name.to_sanitized_upper_case();
            let pc = Ident::new(&*pc_);
            let pc_r = Ident::new(&*format!("{}_R", pc_));
            let pcw = Ident::new(&*format!("{}W", pc_));
            let _pc_w = Ident::new(&*format!("{}_W", pc_));
            let _sc = Ident::new(&*format!("_{}", sc));
            let mut description = if width == 1 {
                format!("Bit {}", offset)
            } else {
                format!("Bits {}:{}", offset, offset + width - 1)
            };
            if let Some(d) = &f.description {
                description.push_str(" - ");
                description.push_str(&*util::respace(&util::escape_brackets(d)));
            }
            Ok(F {
                _pc_w,
                _sc,
                description,
                pc,
                pc_r,
                pcw,
                width,
                access: f.access,
                evs: &f.enumerated_values,
                sc: Ident::from(&*sc),
                mask: util::hex((((1 as u64) << width) - 1) as u32),
                name: &f.name,
                offset: util::unsuffixed(u64::from(f.bit_range.offset)),
                ty: width.to_ty()?,
                write_constraint: f.write_constraint.as_ref(),
            })
        }
    }

    let fs = fields.iter().map(F::from).collect::<Result<Vec<_>>>()?;

    // TODO enumeratedValues
    for f in &fs {
        let can_read = [Access::ReadOnly, Access::ReadWriteOnce, Access::ReadWrite].contains(&access) &&
            (f.access != Some(Access::WriteOnly)) &&
            (f.access != Some(Access::WriteOnce));
        let can_write = (access != Access::ReadOnly) && (f.access != Some(Access::ReadOnly));

        let mask = &f.mask;
        let offset = &f.offset;
        let fty = &f.ty;
        let pc = &f.pc;
        let mut pcw = &f.pc;

        let lookup_results = lookup(
            &f.evs,
            fields,
            parent,
            all_registers,
            peripheral,
            all_peripherals,
        )?;

        if lookup_results.len() > 1 {
            pcw = &f.pcw;
            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Read) {
                if base.is_none() {
                    let variants = Variant::from_enumerated_values(evs)?;
                    add_new_enum(mod_items, &variants, pc, fty, f.name, f.width);
                }
            }
            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Write) {
                if base.is_none() {
                    let variants = Variant::from_enumerated_values(evs)?;
                    add_new_enum(mod_items, &variants, pcw, fty, f.name, f.width);
                }
            }
        } else if lookup_results.len() == 1 {
            let mut used = false;
            if can_read {
                if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Read) {
                    used = true;
                    if base.is_none() {
                        let variants = Variant::from_enumerated_values(evs)?;
                        add_new_enum(mod_items, &variants, pc, fty, f.name, f.width);
                    }
                } else {
                    pcw = &f.pcw;
                    let desc = &f.description;
                    mod_items.push(quote! {
                        #[doc = #desc]
                        pub struct #pc;
                    });
                }
            }
            if can_write {
                if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Write) {
                    if !used && base.is_none() {
                        let variants = Variant::from_enumerated_values(evs)?;
                        add_new_enum(mod_items, &variants, pcw, fty, f.name, f.width);
                    }
                } else {
                    let desc = &f.description;
                    mod_items.push(quote! {
                        #[doc = #desc]
                        pub struct #pcw;
                    });
                }
            }
        } else {
            let desc = &f.description;
            mod_items.push(quote! {
                #[doc = #desc]
                pub struct #pc;
            });
        }

        if can_read {
            let pc_r = &f.pc_r;
            let cast = if f.width == 1 {
                quote! { != 0 }
            } else {
                quote! { as #fty }
            };
            let value = quote! {
                ((self.bits() >> #offset) & #mask) #cast
            };

            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Read) {
                let variants = Variant::from_enumerated_values(evs)?;

                if let Some(base) = &base {
                    let pc = base.field.to_sanitized_upper_case();
                    let base_pc_r = Ident::from(&*format!("{}_R", pc));
                    let desc = format!("Possible values of the field `{}`", f.name,);

                    if let (Some(peripheral), Some(register)) = (&base.peripheral, &base.register) {
                        let pmod_ = peripheral.to_sanitized_snake_case();
                        let rmod_ = register.to_sanitized_snake_case();
                        let pmod_ = Ident::from(&*pmod_);
                        let rmod_ = Ident::from(&*rmod_);

                        mod_items.push(quote! {
                            #[doc = #desc]
                            pub type #pc_r = crate::#pmod_::#rmod_::#base_pc_r;
                        });
                    } else if let Some(register) = &base.register {
                        let mod_ = register.to_sanitized_snake_case();
                        let mod_ = Ident::from(&*mod_);

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

                if base.is_none() {
                    let mut enum_items = vec![];

                    for v in &variants {
                        let pcv = &v.pc;
                        let scv = &v.sc;

                        let is_variant = if scv.as_ref().starts_with('_') {
                            Ident::from(&*format!("is{}", scv))
                        } else {
                            Ident::from(&*format!("is_{}", scv))
                        };

                        let doc = format!("Checks if the value of the field is `{}`", pcv);
                        enum_items.push(quote! {
                            #[doc = #doc]
                            #[inline(always)]
                            pub fn #is_variant(&self) -> bool {
                                *self == #pc::#pcv
                            }
                        });
                    }

                    mod_items.push(quote! {
                        impl #pc_r {
                            #(#enum_items)*
                        }
                    });

                    let desc = format!("Reader for field {}", f.name);
                    mod_items.push(quote! {
                        #[doc = #desc]
                        pub type #pc_r = crate::R<#fty, #pc>;
                        impl crate::Readable for #pc {}
                    });
                }
            } else {
                let desc = format!("Reader for field {}", f.name);
                mod_items.push(quote! {
                    #[doc = #desc]
                    pub type #pc_r = crate::R<#fty, #pc>;
                    impl crate::Readable for #pc {}
                });
            }
            let description = &util::escape_brackets(&f.description);
            let sc = &f.sc;
            r_impl_items.push(quote! {
                #[doc = #description]
                #[inline(always)]
                pub fn #sc(&self) -> #pc_r {
                    #pc_r::new( #value )
                }
            });
        }

        let mut unsafety = unsafety(f.write_constraint, f.width);

        if can_write {
            let _pc_w = &f._pc_w;

            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Write) {
                let variants = Variant::from_enumerated_values(evs)?;

                let pc_w_doc = format!("Values that can be written to the field `{}`", f.name);

                base.as_ref().map(|base| {
                    let pc = base.field.to_sanitized_upper_case();
                    let base_pc_w = Ident::from(&*format!("{}_W", pc));

                    if let (Some(peripheral), Some(register)) = (&base.peripheral, &base.register) {
                        let pmod_ = peripheral.to_sanitized_snake_case();
                        let rmod_ = register.to_sanitized_snake_case();
                        let pmod_ = Ident::from(&*pmod_);
                        let rmod_ = Ident::from(&*rmod_);

                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #_pc_w<'a> = crate::#pmod_::#rmod_::#base_pc_w<'a>;
                        });
                    } else if let Some(register) = &base.register {
                        let mod_ = register.to_sanitized_snake_case();
                        let mod_ = Ident::from(&*mod_);

                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #_pc_w<'a> = super::#mod_::#base_pc_w<'a>;
                        });
                    } else {
                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #_pc_w<'a> = #base_pc_w<'a>;
                        });
                    }
                });

                if base.is_none() {
                    let mut proxy_items = vec![];

                    if variants.len() == 1 << f.width {
                        unsafety = Ident::from("Safe");
                    }

                    let desc = format!("Write proxy for field {}", f.name);
                    mod_items.push(quote! {
                        #[doc = #desc]
                        pub type #_pc_w<'a> = crate::WProxy<'a, #rty, super::#name_pc, #fty, #pcw, crate::#unsafety>;
                    });

                    for v in &variants {
                        let pcv = &v.pc;
                        let scv = &v.sc;

                        let doc = util::escape_brackets(util::respace(&v.doc).as_ref());
                        proxy_items.push(quote! {
                            #[doc = #doc]
                            #[inline(always)]
                            pub fn #scv(self) -> &'a mut _W {
                                self.variant(#pcw::#pcv)
                            }
                        });
                    }

                    mod_items.push(quote! {
                        impl<'a> #_pc_w<'a> {
                            #(#proxy_items)*
                        }
                        impl crate::Writable for #pcw {}
                        impl crate::Variant for #pcw {}
                    });
                    if f.width > 1 {
                        let mask = &f.mask;
                        mod_items.push(quote! {
                            impl crate::Mask<#rty> for #pcw {
                                const MASK: #rty = #mask;
                            }
                        });
                    }
                }
            } else {
                let desc = format!("Write proxy for field {}", f.name);
                mod_items.push(quote! {
                    #[doc = #desc]
                    pub type #_pc_w<'a> = crate::WProxy<'a, #rty, super::#name_pc, #fty, #pcw, crate::#unsafety>;
                        impl crate::Writable for #pcw {}
                });
                if f.width > 1 {
                    let mask = &f.mask;
                    mod_items.push(quote! {
                        impl crate::Mask<#rty> for #pcw {
                            const MASK: #rty = #mask;
                        }
                    });
                }
            }

            let description = &util::escape_brackets(&f.description);
            let sc = &f.sc;
            let offset = &f.offset;
            w_impl_items.push(quote! {
                #[doc = #description]
                #[inline(always)]
                pub fn #sc(&mut self) -> #_pc_w {
                    #_pc_w::new( self, #offset )
                }
            })
        }
    }

    Ok(())
}

fn unsafety(write_constraint: Option<&WriteConstraint>, width: u32) -> Ident {
    Ident::from(match &write_constraint {
        Some(&WriteConstraint::Range(range))
            if u64::from(range.min) == 0 && u64::from(range.max) == (1u64 << width) - 1 =>
        {
            // the SVD has acknowledged that it's safe to write
            // any value that can fit in the field
            "Safe"
        }
        None if width == 1 => {
            // the field is one bit wide, so we assume it's legal to write
            // either value into it or it wouldn't exist; despite that
            // if a writeConstraint exists then respect it
            "Safe"
        }
        _ => "Unsafe",
    })
}

struct Variant {
    doc: String,
    pc: Ident,
    sc: Ident,
    value: u64,
}

impl Variant {
    fn from_enumerated_values(evs: &EnumeratedValues) -> Result<Vec<Self>> {
        evs.values
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
                    pc: Ident::from(&*ev.name
                                   .to_sanitized_upper_case()),
                    sc: Ident::from(&*ev.name
                                   .to_sanitized_snake_case()),
                    value,
                })
                },
            )
            .collect::<Result<Vec<_>>>()
    }
}

fn add_new_enum(mod_items: &mut Vec<Tokens>, variants: &Vec<Variant>, pc: &Ident, fty: &Ident, name: &str, width: u32) {
    let pc_doc = format!("Values that can be written to the field `{}`", name);
    let variants_pc = variants.iter().map(|v| &v.pc);
    let variants_doc = variants.iter().map(|v| util::escape_brackets(&v.doc).to_owned());

    let arms = variants.iter().map(|v| {
        let pcv = &v.pc;
        let value = util::unsuffixed_or_bool(v.value, width);

        quote! {
            #pc::#pcv => #value
        }
    });

    mod_items.push(quote! {
        #[doc = #pc_doc]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum #pc {
            #(#[doc = #variants_doc]
            #variants_pc),*
        }
        impl crate::ToBits<#fty> for #pc {
            #[inline(always)]
            fn _bits(&self) -> #fty {
                match *self {
                    #(#arms),*
                }
            }
        }
    });
}

#[derive(Clone, Debug, PartialEq)]
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
    let evs = evs.iter()
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

fn lookup_filter<'a> (evs: &Vec<(&'a EnumeratedValues, Option<Base<'a>>)>, usage: Usage) -> Option<(&'a EnumeratedValues, Option<Base<'a>>)> {
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
                .map(|(f, _)| &f.name)
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
