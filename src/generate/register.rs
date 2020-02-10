use crate::modules::Module;
use crate::quote::ToTokens;
use crate::svd::{
    Access, BitRange, EnumeratedValues, Field, Peripheral, Register, RegisterCluster,
    RegisterProperties, Usage, WriteConstraint,
};
use cast::u64;
use log::warn;
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};

use crate::errors::*;
use crate::util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase, U32Ext};

pub fn render(
    register: &Register,
    all_registers: &[&Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    defs: &RegisterProperties,
) -> Result<Module> {
    let access = util::access_of(register);
    let name = util::name_of(register);
    let span = Span::call_site();
    let name_pc = Ident::new(&name.to_sanitized_upper_case(), span);
    let _name_pc = Ident::new(&format!("_{}", &name.to_sanitized_upper_case()), span);
    let name_sc = name.to_sanitized_snake_case();
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

    let mut module = Module::new(&name_sc, &description);
    let mut r_impl_items = TokenStream::new();
    let mut w_impl_items = TokenStream::new();
    let mut methods = vec![];

    let can_read = [Access::ReadOnly, Access::ReadWriteOnce, Access::ReadWrite].contains(&access);
    let can_write = access != Access::ReadOnly;

    if can_read {
        let desc = format!("Reader of register {}", register.name);
        module.extend(quote! {
            #[doc = #desc]
            pub type R = crate::R<#rty, super::#name_pc>;
        });
        methods.push("read");
    }

    let res_val = register.reset_value.or(defs.reset_value).map(|v| v as u64);
    if can_write {
        let desc = format!("Writer for register {}", register.name);
        module.extend(quote! {
            #[doc = #desc]
            pub type W = crate::W<#rty, super::#name_pc>;
        });
        if let Some(rv) = res_val.map(util::hex) {
            let doc = format!("Register {} `reset()`'s with value {}", register.name, &rv);
            module.extend(quote! {
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
                &mut module,
                &mut r_impl_items,
                &mut w_impl_items,
            )?;
        }
    }

    let open = Punct::new('{', Spacing::Alone);
    let close = Punct::new('}', Spacing::Alone).into_token_stream();

    if can_read {
        module.extend(quote! {
            impl R #open
        });

        module.extend(r_impl_items);

        module.extend(close.clone());
    }

    if can_write {
        module.extend(quote! {
            impl W #open
        });

        module.extend(w_impl_items);

        module.extend(close);
    }

    let methods = methods
        .iter()
        .map(|s| format!("[`{0}`](crate::generic::Reg::{0})", s))
        .collect::<Vec<_>>();
    let mut doc = format!("{}\n\nThis register you can {}. See [API](https://docs.rs/svd2rust/#read--modify--write-api).",
                        &description, methods.join(", "));

    if name_sc != "cfg" {
        doc += format!(
            "\n\nFor information about available fields see [{0}]({0}) module",
            &name_sc
        )
        .as_str();
    }
    module.out.extend(quote! {
        #[doc = #doc]
        pub type #name_pc = crate::Reg<#rty, #_name_pc>;

        #[allow(missing_docs)]
        #[doc(hidden)]
        pub struct #_name_pc;
    });

    if can_read {
        let doc = format!(
            "`read()` method returns [{0}::R]({0}::R) reader structure",
            &name_sc
        );
        module.out.extend(quote! {
            #[doc = #doc]
            impl crate::Readable for #name_pc {}
        });
    }
    if can_write {
        let doc = format!(
            "`write(|w| ..)` method takes [{0}::W]({0}::W) writer structure",
            &name_sc
        );
        module.out.extend(quote! {
            #[doc = #doc]
            impl crate::Writable for #name_pc {}
        });
    }
    Ok(module)
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
    module: &mut Module,
    r_impl_items: &mut TokenStream,
    w_impl_items: &mut TokenStream,
) -> Result<()> {
    let span = Span::call_site();
    let can_read = [Access::ReadOnly, Access::ReadWriteOnce, Access::ReadWrite].contains(&access);
    let can_write = access != Access::ReadOnly;

    // TODO enumeratedValues
    let inline = quote! { #[inline(always)] };
    for f in fields.iter() {
        // TODO(AJM) - do we need to do anything with this range type?
        let BitRange { offset, width, .. } = f.bit_range;
        let name = util::replace_suffix(&f.name, "");
        let sc = Ident::new(&name.to_sanitized_snake_case(), span);
        let pc = name.to_sanitized_upper_case();
        let bits = Ident::new(if width == 1 { "bit" } else { "bits" }, span);
        let description = if let Some(d) = &f.description {
            util::respace(&util::escape_brackets(d))
        } else {
            "".to_owned()
        };

        let can_read = can_read
            && (f.access != Some(Access::WriteOnly))
            && (f.access != Some(Access::WriteOnce));
        let can_write = can_write && (f.access != Some(Access::ReadOnly));

        let mask = 1u64.wrapping_neg() >> (64 - width);
        let hexmask = &util::hex(mask);
        let offset = u64::from(offset);
        let rv = reset_value.map(|rv| (rv >> offset) & mask);
        let fty = width.to_ty()?;
        let evs = &f.enumerated_values;

        let lookup_results = lookup(
            evs,
            fields,
            parent,
            all_registers,
            peripheral,
            all_peripherals,
        )?;

        // Reader and writer use one common `Enum_A` unless a fields have two `enumeratedValues`,
        // then we have one for read-only `Enum_A` and another for write-only `Enum_AW`
        let pc_r = Ident::new(&(pc.clone() + "_A"), span);
        let mut pc_w = &pc_r;

        let mut evs_r = None;

        let field_dim = match f {
            Field::Array(_, de) => {
                let (first, index) = if let Some(dim_index) = &de.dim_index {
                    if let Ok(first) = dim_index[0].parse::<u32>() {
                        let sequential_indexes = dim_index
                            .iter()
                            .map(|element| element.parse::<u32>())
                            .eq((first..de.dim + first).map(Ok));
                        if !sequential_indexes {
                            return Err(format!("unsupported array indexes in {}", f.name))?;
                        }
                        (first, None)
                    } else {
                        (0, de.dim_index.clone())
                    }
                } else {
                    (0, None)
                };
                let suffixes: Vec<_> = match index {
                    Some(ix) => ix,
                    None => (0..de.dim).map(|i| (first + i).to_string()).collect(),
                };
                let suffixes_str = format!("({}-{})", first, first + de.dim - 1);
                Some((first, de.dim, de.dim_increment, suffixes, suffixes_str))
            }
            Field::Single(_) => {
                if f.name.contains("%s") {
                    return Err(format!("incorrect field {}", f.name))?;
                }
                None
            }
        };

        if can_read {
            let readerdoc = if let Some((_, _, _, _, suffixes_str)) = &field_dim {
                format!(
                    "Reader of fields `{}`",
                    util::replace_suffix(&f.name, suffixes_str)
                )
            } else {
                let quotedfield = String::from("`") + &f.name + "`";
                String::from("Reader of field ") + &quotedfield
            };

            let _pc_r = Ident::new(&(pc.clone() + "_R"), span);

            let cast = if width == 1 {
                quote! { != 0 }
            } else {
                quote! { as #fty }
            };
            let value = if offset != 0 {
                let offset = &util::unsuffixed(offset);
                quote! {
                    ((self.bits >> #offset) & #hexmask) #cast
                }
            } else {
                quote! {
                    (self.bits & #hexmask) #cast
                }
            };

            if let Some((first, dim, increment, suffixes, suffixes_str)) = &field_dim {
                let offset_calc = calculate_offset(*first, *increment, offset);
                let value = quote! { ((self.bits >> #offset_calc) & #hexmask) #cast };
                let doc = &util::replace_suffix(&description, suffixes_str);
                r_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub unsafe fn #sc(&self, n: usize) -> #_pc_r {
                        #_pc_r::new ( #value )
                    }
                });
                for (i, suffix) in (0..*dim).zip(suffixes.iter()) {
                    let sub_offset = offset + (i as u64) * (*increment as u64);
                    let value = if sub_offset != 0 {
                        let sub_offset = &util::unsuffixed(sub_offset);
                        quote! {
                            ((self.bits >> #sub_offset) & #hexmask) #cast
                        }
                    } else {
                        quote! {
                            (self.bits & #hexmask) #cast
                        }
                    };
                    let sc_n = Ident::new(
                        &util::replace_suffix(&f.name.to_sanitized_snake_case(), &suffix),
                        Span::call_site(),
                    );
                    let doc = util::replace_suffix(
                        &description_with_bits(&description, sub_offset, width),
                        &suffix,
                    );
                    r_impl_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        pub fn #sc_n(&self) -> #_pc_r {
                            #_pc_r::new ( #value )
                        }
                    });
                }
            } else {
                let doc = description_with_bits(&description, offset, width);
                r_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub fn #sc(&self) -> #_pc_r {
                        #_pc_r::new ( #value )
                    }
                });
            }

            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Read) {
                evs_r = Some(evs.clone());

                if let Some(base) = base {
                    let pc = util::replace_suffix(base.field, "");
                    let pc = pc.to_sanitized_upper_case();
                    let base_pc_r = Ident::new(&(pc + "_A"), span);
                    derive_from_base(module, &base, &pc_r, &base_pc_r, &description);

                    module.extend(quote! {
                        #[doc = #readerdoc]
                        pub type #_pc_r = crate::R<#fty, #pc_r>;
                    });
                } else {
                    let has_reserved_variant = evs.values.len() != (1 << width);
                    let variants = Variant::from_enumerated_values(evs)?;

                    add_from_variants(module, &variants, &pc_r, &fty, &description, rv);

                    let mut enum_items = TokenStream::new();

                    let mut arms = TokenStream::new();
                    for v in variants.iter().map(|v| {
                        let i = util::unsuffixed_or_bool(v.value, width);
                        let pc = &v.pc;

                        if has_reserved_variant {
                            quote! { #i => Val(#pc_r::#pc), }
                        } else {
                            quote! { #i => #pc_r::#pc, }
                        }
                    }) {
                        arms.extend(v);
                    }

                    if has_reserved_variant {
                        arms.extend(quote! {
                            i => Res(i),
                        });
                    } else if 1 << width.to_ty_width()? != variants.len() {
                        arms.extend(quote! {
                            _ => unreachable!(),
                        });
                    }

                    if has_reserved_variant {
                        enum_items.extend(quote! {
                            ///Get enumerated values variant
                            #inline
                            pub fn variant(&self) -> crate::Variant<#fty, #pc_r> {
                                use crate::Variant::*;
                                match self.bits {
                                    #arms
                                }
                            }
                        });
                    } else {
                        enum_items.extend(quote! {
                            ///Get enumerated values variant
                            #inline
                            pub fn variant(&self) -> #pc_r {
                                match self.bits {
                                    #arms
                                }
                            }
                        });
                    }

                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.sc;

                        let is_variant = Ident::new(
                            &if sc.to_string().starts_with('_') {
                                format!("is{}", sc)
                            } else {
                                format!("is_{}", sc)
                            },
                            span,
                        );

                        let doc = format!("Checks if the value of the field is `{}`", pc);
                        enum_items.extend(quote! {
                            #[doc = #doc]
                            #inline
                            pub fn #is_variant(&self) -> bool {
                                *self == #pc_r::#pc
                            }
                        });
                    }

                    module.extend(quote! {
                        #[doc = #readerdoc]
                        pub type #_pc_r = crate::R<#fty, #pc_r>;
                        impl #_pc_r {
                            #enum_items
                        }
                    });
                }
            } else {
                module.extend(quote! {
                    #[doc = #readerdoc]
                    pub type #_pc_r = crate::R<#fty, #fty>;
                })
            }
        }

        if can_write {
            let new_pc_w = Ident::new(&(pc.clone() + "_AW"), span);
            let _pc_w = Ident::new(&(pc.clone() + "_W"), span);

            let mut proxy_items = TokenStream::new();
            let mut unsafety = unsafety(f.write_constraint.as_ref(), width);

            if let Some((evs, base)) = lookup_filter(&lookup_results, Usage::Write) {
                let variants = Variant::from_enumerated_values(evs)?;

                if variants.len() == 1 << width {
                    unsafety = None;
                }

                if Some(evs) != evs_r.as_ref() {
                    pc_w = &new_pc_w;
                    if let Some(base) = base {
                        let pc = util::replace_suffix(base.field, "");
                        let pc = pc.to_sanitized_upper_case();
                        let base_pc_w = Ident::new(&(pc + "_AW"), span);
                        derive_from_base(module, &base, &pc_w, &base_pc_w, &description)
                    } else {
                        add_from_variants(module, &variants, pc_w, &fty, &description, rv);
                    }
                }

                proxy_items.extend(quote! {
                    ///Writes `variant` to the field
                    #inline
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
                    proxy_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        pub fn #sc(self) -> &'a mut W {
                            self.variant(#pc_w::#pc)
                        }
                    });
                }
            }

            if width == 1 {
                proxy_items.extend(quote! {
                    ///Sets the field bit
                    #inline
                    pub #unsafety fn set_bit(self) -> &'a mut W {
                        self.bit(true)
                    }

                    ///Clears the field bit
                    #inline
                    pub #unsafety fn clear_bit(self) -> &'a mut W {
                        self.bit(false)
                    }
                });
            }

            proxy_items.extend(if field_dim.is_some() {
                quote! {
                    ///Writes raw bits to the field
                    #inline
                    pub #unsafety fn #bits(self, value: #fty) -> &'a mut W {
                        self.w.bits = (self.w.bits & !(#hexmask << self.offset)) | (((value as #rty) & #hexmask) << self.offset);
                        self.w
                    }
                }
            } else if offset != 0 {
                let offset = &util::unsuffixed(offset);
                quote! {
                    ///Writes raw bits to the field
                    #inline
                    pub #unsafety fn #bits(self, value: #fty) -> &'a mut W {
                        self.w.bits = (self.w.bits & !(#hexmask << #offset)) | (((value as #rty) & #hexmask) << #offset);
                        self.w
                    }
                }
            } else {
                quote! {
                    ///Writes raw bits to the field
                    #inline
                    pub #unsafety fn #bits(self, value: #fty) -> &'a mut W {
                        self.w.bits = (self.w.bits & !#hexmask) | ((value as #rty) & #hexmask);
                        self.w
                    }
                }
            });

            let doc;
            let offset_entry;
            if let Some((_, _, _, _, suffixes_str)) = &field_dim {
                doc = format!(
                    "Write proxy for fields `{}`",
                    util::replace_suffix(&f.name, suffixes_str)
                );
                offset_entry = quote! {offset: usize,};
            } else {
                doc = format!("Write proxy for field `{}`", f.name);
                offset_entry = quote! {};
            }

            module.extend(quote! {
                #[doc = #doc]
                pub struct #_pc_w<'a> {
                    w: &'a mut W,
                    #offset_entry
                }

                impl<'a> #_pc_w<'a> {
                    #proxy_items
                }
            });

            if let Some((first, dim, increment, suffixes, suffixes_str)) = &field_dim {
                let offset_calc = calculate_offset(*first, *increment, offset);
                let doc = &util::replace_suffix(&description, suffixes_str);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub unsafe fn #sc(&mut self, n: usize) -> #_pc_w {
                        #_pc_w { w: self, offset: #offset_calc }
                    }
                });
                for (i, suffix) in (0..*dim).zip(suffixes.iter()) {
                    let sub_offset = offset + (i as u64) * (*increment as u64);
                    let sc_n = Ident::new(
                        &util::replace_suffix(&f.name.to_sanitized_snake_case(), &suffix),
                        Span::call_site(),
                    );
                    let doc = util::replace_suffix(
                        &description_with_bits(&description, sub_offset, width),
                        &suffix,
                    );
                    let sub_offset = util::unsuffixed(sub_offset as u64);
                    w_impl_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        pub fn #sc_n(&mut self) -> #_pc_w {
                            #_pc_w { w: self, offset: #sub_offset }
                        }
                    });
                }
            } else {
                let doc = description_with_bits(&description, offset, width);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub fn #sc(&mut self) -> #_pc_w {
                        #_pc_w { w: self }
                    }
                });
            }
        }
    }

    Ok(())
}

fn unsafety(write_constraint: Option<&WriteConstraint>, width: u32) -> Option<Ident> {
    match &write_constraint {
        Some(&WriteConstraint::Range(range))
            if u64::from(range.min) == 0
                && u64::from(range.max) == 1u64.wrapping_neg() >> (64 - width) =>
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

fn add_from_variants(
    module: &mut Module,
    variants: &[Variant],
    pc: &Ident,
    fty: &Ident,
    desc: &str,
    reset_value: Option<u64>,
) {
    let (repr, cast) = if fty == "bool" {
        (quote! {}, quote! { variant as u8 != 0 })
    } else {
        (quote! { #[repr(#fty)] }, quote! { variant as _ })
    };

    let mut vars = TokenStream::new();
    for v in variants.iter().map(|v| {
        let desc = util::escape_brackets(&format!("{}: {}", v.value, v.doc));
        let pcv = &v.pc;
        let pcval = &util::unsuffixed(v.value);
        quote! {
            #[doc = #desc]
            #pcv = #pcval,
        }
    }) {
        vars.extend(v);
    }

    let desc = if let Some(rv) = reset_value {
        format!("{}\n\nValue on reset: {}", desc, rv)
    } else {
        desc.to_owned()
    };

    module.extend(quote! {
        #[doc = #desc]
        #[derive(Clone, Copy, Debug, PartialEq)]
        #repr
        pub enum #pc {
            #vars
        }
        impl From<#pc> for #fty {
            #[inline(always)]
            fn from(variant: #pc) -> Self {
                #cast
            }
        }
    });
}

fn calculate_offset(first: u32, increment: u32, offset: u64) -> TokenStream {
    let mut res = if first != 0 {
        let first = util::unsuffixed(first as u64);
        quote! { n - #first }
    } else {
        quote! { n }
    };
    if increment != 1 {
        let increment = util::unsuffixed(increment as u64);
        res = if first != 0 {
            quote! { (#res) * #increment }
        } else {
            quote! { #res * #increment }
        };
    }
    if offset != 0 {
        let offset = &util::unsuffixed(offset);
        res = quote! { #res + #offset };
    }
    res
}

fn description_with_bits(description: &str, offset: u64, width: u32) -> String {
    let mut res = if width == 1 {
        format!("Bit {}", offset)
    } else {
        format!("Bits {}:{}", offset, offset + width as u64 - 1)
    };
    if description.len() > 0 {
        res.push_str(" - ");
        res.push_str(&util::respace(&util::escape_brackets(description)));
    }
    res
}

fn derive_from_base(module: &mut Module, base: &Base, pc: &Ident, base_pc: &Ident, desc: &str) {
    let span = Span::call_site();
    if let (Some(peripheral), Some(register)) = (&base.peripheral, &base.register) {
        let pmod_ = peripheral.to_sanitized_snake_case();
        let rmod_ = register.to_sanitized_snake_case();
        let pmod_ = Ident::new(&pmod_, span);
        let rmod_ = Ident::new(&rmod_, span);

        module.extend(quote! {
            #[doc = #desc]
            pub type #pc =
                crate::#pmod_::#rmod_::#base_pc;
        });
    } else if let Some(register) = &base.register {
        let mod_ = register.to_sanitized_snake_case();
        let mod_ = Ident::new(&mod_, span);

        module.extend(quote! {
            #[doc = #desc]
            pub type #pc =
                super::#mod_::#base_pc;
        });
    } else {
        module.extend(quote! {
            #[doc = #desc]
            pub type #pc = #base_pc;
        });
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
    evs: &[(&'a EnumeratedValues, Option<Base<'a>>)],
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
        lookup_in_field(base_evs, None, None, base_field)
    } else {
        Err(format!(
            "Field {} not found in register {}",
            base_field, register.name
        )
        .into())
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
            Err(format!("No field {} in register {}", base_field, register.name).into())
        }
    } else {
        Err(format!(
            "No register {} in peripheral {}",
            base_register, peripheral.name
        )
        .into())
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

    Err(format!("No EnumeratedValues {} in field {}", base_evs, field.name).into())
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
        )
        .into()),
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
                )
                .into())
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
        Err(format!("No peripheral {}", base_peripheral).into())
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
