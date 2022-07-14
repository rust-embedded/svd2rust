use crate::svd::{
    Access, BitRange, EnumeratedValues, Field, ModifiedWriteValues, Peripheral, ReadAction,
    Register, RegisterProperties, Usage, WriteConstraint,
};
use cast::u64;
use core::u64;
use log::warn;
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};
use quote::{quote, ToTokens};

use crate::util::{self, Config, ToSanitizedCase, U32Ext};
use anyhow::{anyhow, Result};

pub fn render(
    register: &Register,
    all_registers: &[&Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    config: &Config,
) -> Result<TokenStream> {
    let properties = &register.properties;
    let access = util::access_of(properties, register.fields.as_deref());
    let name = util::name_of(register, config.ignore_groups);
    let span = Span::call_site();
    let name_constant_case = Ident::new(&name.to_sanitized_constant_case(), span);
    let name_constant_case_spec = Ident::new(
        &format!("{}_SPEC", &name.to_sanitized_constant_case()),
        span,
    );
    let name_snake_case = Ident::new(&name.to_sanitized_snake_case(), span);
    let rsize = properties
        .size
        .ok_or_else(|| anyhow!("Register {} has no `size` field", register.name))?;
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
            Default::default()
        }))
        .as_ref(),
    );

    let mut mod_items = TokenStream::new();
    let mut r_impl_items = TokenStream::new();
    let mut w_impl_items = TokenStream::new();
    let mut methods = vec![];

    let can_read = access.can_read();
    let can_write = access.can_write();
    let can_reset = properties.reset_value.is_some();

    if can_read {
        let desc = format!("Register `{}` reader", register.name);
        let derive = if config.derive_more {
            Some(quote! { #[derive(derive_more::Deref, derive_more::From)] })
        } else {
            None
        };
        mod_items.extend(quote! {
            #[doc = #desc]
            #derive
            pub struct R(crate::R<#name_constant_case_spec>);
        });

        if !config.derive_more {
            mod_items.extend(quote! {
                impl core::ops::Deref for R {
                    type Target = crate::R<#name_constant_case_spec>;

                    #[inline(always)]
                    fn deref(&self) -> &Self::Target {
                        &self.0
                    }
                }

                impl From<crate::R<#name_constant_case_spec>> for R {
                    #[inline(always)]
                    fn from(reader: crate::R<#name_constant_case_spec>) -> Self {
                        R(reader)
                    }
                }
            });
        }
        methods.push("read");
    }

    if can_write {
        let desc = format!("Register `{}` writer", register.name);
        let derive = if config.derive_more {
            Some(quote! { #[derive(derive_more::Deref, derive_more::DerefMut, derive_more::From)] })
        } else {
            None
        };
        mod_items.extend(quote! {
            #[doc = #desc]
            #derive
            pub struct W(crate::W<#name_constant_case_spec>);
        });

        if !config.derive_more {
            mod_items.extend(quote! {
                impl core::ops::Deref for W {
                    type Target = crate::W<#name_constant_case_spec>;

                    #[inline(always)]
                    fn deref(&self) -> &Self::Target {
                        &self.0
                    }
                }

                impl core::ops::DerefMut for W {
                    #[inline(always)]
                    fn deref_mut(&mut self) -> &mut Self::Target {
                        &mut self.0
                    }
                }

                impl From<crate::W<#name_constant_case_spec>> for W {
                    #[inline(always)]
                    fn from(writer: crate::W<#name_constant_case_spec>) -> Self {
                        W(writer)
                    }
                }
            });
        }
        methods.push("write_with_zero");
        if can_reset {
            methods.push("reset");
            methods.push("write");
        }
    }

    if can_read && can_write {
        methods.push("modify");
    }

    if let Some(cur_fields) = register.fields.as_ref() {
        // filter out all reserved fields, as we should not generate code for
        // them
        let cur_fields: Vec<&Field> = cur_fields
            .iter()
            .filter(|field| field.name.to_lowercase() != "reserved")
            .collect();

        if !cur_fields.is_empty() {
            fields(
                cur_fields,
                register,
                &name_constant_case_spec,
                all_registers,
                peripheral,
                all_peripherals,
                &rty,
                access,
                properties,
                &mut mod_items,
                &mut r_impl_items,
                &mut w_impl_items,
                config,
            )?;
        }
    }

    let open = Punct::new('{', Spacing::Alone);
    let close = Punct::new('}', Spacing::Alone);

    if can_read && !r_impl_items.is_empty() {
        mod_items.extend(quote! {
            impl R #open #r_impl_items #close
        });
    }

    if can_write {
        mod_items.extend(quote! {
            impl W #open
        });

        mod_items.extend(w_impl_items);

        // the writer can be safe if:
        // * there is a single field that covers the entire register
        // * that field can represent all values
        let can_write_safe = match register
            .fields
            .as_ref()
            .and_then(|fields| fields.iter().next())
            .and_then(|field| field.write_constraint)
        {
            Some(WriteConstraint::Range(range)) => {
                range.min == 0 && range.max == u64::MAX >> (64 - rsize)
            }
            _ => false,
        };

        if can_write_safe {
            mod_items.extend(quote! {
                #[doc = "Writes raw bits to the register."]
                #[inline(always)]
                pub fn bits(&mut self, bits: #rty) -> &mut Self {
                    unsafe { self.0.bits(bits) };
                    self
                }
            });
        } else {
            mod_items.extend(quote! {
                #[doc = "Writes raw bits to the register."]
                #[inline(always)]
                pub unsafe fn bits(&mut self, bits: #rty) -> &mut Self {
                    self.0.bits(bits);
                    self
                }
            });
        }

        close.to_tokens(&mut mod_items);
    }

    let mut out = TokenStream::new();
    let methods = methods
        .iter()
        .map(|s| format!("[`{0}`](crate::generic::Reg::{0})", s))
        .collect::<Vec<_>>();
    let mut doc = format!("{}\n\nThis register you can {}. See [API](https://docs.rs/svd2rust/#read--modify--write-api).",
                        &description, methods.join(", "));

    if name_snake_case != "cfg" {
        doc += format!(
            "\n\nFor information about available fields see [{0}](index.html) module",
            &name_snake_case
        )
        .as_str();
    }

    if can_read {
        if let Some(action) = register.read_action {
            doc += match action {
                ReadAction::Clear => "\n\nThe register is **cleared** (set to zero) following a read operation.",
                ReadAction::Set => "\n\nThe register is **set** (set to ones) following a read operation.",
                ReadAction::Modify => "\n\nThe register is **modified** in some way after a read operation.",
                ReadAction::ModifyExternal => "\n\nOne or more dependent resources other than the current register are immediately affected by a read operation.",
            };
        }
    }

    let alias_doc = format!(
        "{} register accessor: an alias for `Reg<{}>`",
        name, name_constant_case_spec,
    );
    out.extend(quote! {
        #[doc = #alias_doc]
        pub type #name_constant_case = crate::Reg<#name_snake_case::#name_constant_case_spec>;
    });
    mod_items.extend(quote! {
        #[doc = #doc]
        pub struct #name_constant_case_spec;

        impl crate::RegisterSpec for #name_constant_case_spec {
            type Ux = #rty;
        }
    });

    if can_read {
        let doc = format!(
            "`read()` method returns [{0}::R](R) reader structure",
            &name_snake_case
        );
        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Readable for #name_constant_case_spec {
                type Reader = R;
            }
        });
    }
    if can_write {
        let doc = format!(
            "`write(|w| ..)` method takes [{0}::W](W) writer structure",
            &name_snake_case
        );
        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Writable for #name_constant_case_spec {
                type Writer = W;
            }
        });
    }
    if let Some(rv) = properties.reset_value.map(util::hex) {
        let doc = format!("`reset()` method sets {} to value {}", register.name, &rv);
        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Resettable for #name_constant_case_spec {
                #[inline(always)]
                fn reset_value() -> Self::Ux { #rv }
            }
        });
    }

    out.extend(quote! {
        #[doc = #description]
        pub mod #name_snake_case #open
    });

    out.extend(mod_items);

    out.extend(quote! {
        #close
    });

    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub fn fields(
    mut fields: Vec<&Field>,
    register: &Register,
    name_constant_case_spec: &Ident,
    all_registers: &[&Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    rty: &Ident,
    access: Access,
    properties: &RegisterProperties,
    mod_items: &mut TokenStream,
    r_impl_items: &mut TokenStream,
    w_impl_items: &mut TokenStream,
    config: &Config,
) -> Result<()> {
    let span = Span::call_site();
    let can_read = access.can_read();
    let can_write = access.can_write();

    fields.sort_by_key(|f| f.bit_offset());

    // TODO enumeratedValues
    let inline = quote! { #[inline(always)] };
    for f in fields.iter() {
        // TODO(AJM) - do we need to do anything with this range type?
        let BitRange { offset, width, .. } = f.bit_range;
        let name = util::replace_suffix(&f.name, "");
        let name_snake_case = Ident::new(&name.to_sanitized_snake_case(), span);
        let name_constant_case = name.to_sanitized_constant_case();
        let description_raw = f.description.as_deref().unwrap_or(""); // raw description, if absent using empty string
        let description = util::respace(&util::escape_brackets(description_raw));

        let can_read = can_read
            && (f.access != Some(Access::WriteOnly))
            && (f.access != Some(Access::WriteOnce));
        let can_write = can_write && (f.access != Some(Access::ReadOnly));

        let mask = u64::MAX >> (64 - width);
        let hexmask = &util::digit_or_hex(mask);
        let offset = u64::from(offset);
        let rv = properties.reset_value.map(|rv| (rv >> offset) & mask);
        let fty = width.to_ty()?;
        let evs = &f.enumerated_values;

        let use_mask = if let Some(size) = properties.size {
            size != width
        } else {
            true
        };

        let lookup_results = lookup(
            evs,
            &fields,
            register,
            all_registers,
            peripheral,
            all_peripherals,
        )?;

        let mut evs_r = None;

        // Reads dim information from svd field. If it has dim index, the field is treated as an
        // array; or it should be treated as a single register field.
        let field_dim = match f {
            Field::Array(_, de) => {
                let first = if let Some(dim_index) = &de.dim_index {
                    if let Ok(first) = dim_index[0].parse::<u32>() {
                        let sequential_indexes = dim_index
                            .iter()
                            .map(|element| element.parse::<u32>())
                            .eq((first..de.dim + first).map(Ok));
                        if !sequential_indexes {
                            return Err(anyhow!("unsupported array indexes in {}", f.name));
                        }
                        first
                    } else {
                        0
                    }
                } else {
                    0
                };
                let suffixes: Vec<_> = de.indexes().collect();
                let suffixes_str = format!("({}-{})", first, first + de.dim - 1);
                Some((first, de.dim, de.dim_increment, suffixes, suffixes_str))
            }
            Field::Single(_) => {
                if f.name.contains("%s") {
                    return Err(anyhow!("incorrect field {}", f.name));
                }
                None
            }
        };

        // If this field can be read, generate read proxy structure and value structure.
        if can_read {
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
            } else if use_mask {
                quote! {
                    (self.bits & #hexmask) #cast
                }
            } else {
                quote! {
                    self.bits
                }
            };

            // get a brief description for this field
            // the suffix string from field name is removed in brief description.
            let field_reader_brief = if let Some((_, _, _, _, suffixes_str)) = &field_dim {
                format!(
                    "Fields `{}` reader - {}",
                    util::replace_suffix(&f.name, suffixes_str),
                    description,
                )
            } else {
                format!("Field `{}` reader - {}", f.name, description)
            };

            // get the type of value structure. It can be generated from either name field
            // in enumeratedValues if it's an enumeration, or from field name directly if it's not.
            let value_read_ty = if let Some((evs, _)) = lookup_filter(&lookup_results, Usage::Read)
            {
                if let Some(enum_name) = &evs.name {
                    let enum_name_constant_case = enum_name.to_sanitized_constant_case();
                    let enum_value_read_ty =
                        Ident::new(&format!("{}_A", enum_name_constant_case), span);
                    enum_value_read_ty
                } else {
                    let derived_field_value_read_ty =
                        Ident::new(&format!("{}_A", name_constant_case), span);
                    derived_field_value_read_ty
                }
            } else {
                let raw_field_value_read_ty = fty.clone();
                raw_field_value_read_ty
            };

            // name of read proxy type
            let reader_ty = Ident::new(&(name_constant_case.clone() + "_R"), span);

            // if it's enumeratedValues and it's derived from base, don't derive the read proxy
            // as the base has already dealt with this;
            // if it's enumeratedValues but not derived from base, derive the reader from
            // information in enumeratedValues;
            // if it's not enumeratedValues, always derive the read proxy as we do not need to re-export
            // it again from BitReader or FieldReader.
            let should_derive_reader = match lookup_filter(&lookup_results, Usage::Read) {
                Some((_evs, Some(_base))) => false,
                Some((_evs, None)) => true,
                None => true,
            };

            // derive the read proxy structure if necessary.
            if should_derive_reader {
                let reader = if width == 1 {
                    quote! { crate::BitReader<#value_read_ty> }
                } else {
                    quote! { crate::FieldReader<#fty, #value_read_ty> }
                };
                let mut readerdoc = field_reader_brief.clone();
                if let Some(action) = f.read_action {
                    readerdoc += match action {
                        ReadAction::Clear => "\n\nThe field is **cleared** (set to zero) following a read operation.",
                        ReadAction::Set => "\n\nThe field is **set** (set to ones) following a read operation.",
                        ReadAction::Modify => "\n\nThe field is **modified** in some way after a read operation.",
                        ReadAction::ModifyExternal => "\n\nOne or more dependent resources other than the current field are immediately affected by a read operation.",
                    };
                }
                mod_items.extend(quote! {
                    #[doc = #readerdoc]
                    pub type #reader_ty = #reader;
                });
            }

            // collect information on items in enumeration to generate it later.
            let mut enum_items = TokenStream::new();

            // if this is an enumeratedValues not derived from base, generate the enum structure
            // and implement functions for each value in enumeration.
            if let Some((evs, None)) = lookup_filter(&lookup_results, Usage::Read) {
                // we have enumeration for read, record this. If the enumeration for write operation
                // later on is the same as the read enumeration, we reuse and do not generate again.
                evs_r = Some(evs.clone());

                // do we have finite definition of this enumeration in svd? If not, the later code would
                // return an Option when the value read from field does not match any defined values.
                let has_reserved_variant = evs.values.len() != (1 << width);
                // parse enum variants from enumeratedValues svd record
                let variants = Variant::from_enumerated_values(evs, config.pascal_enum_values)?;

                // if there's no variant defined in enumeratedValues, generate enumeratedValues with new-type
                // wrapper struct, and generate From conversation only.
                // else, generate enumeratedValues into a Rust enum with functions for each variant.
                if variants.is_empty() {
                    // generate struct VALUE_READ_TY_A(fty) and From<fty> for VALUE_READ_TY_A.
                    add_with_no_variants(mod_items, &value_read_ty, &fty, &description, rv);
                } else {
                    // generate enum VALUE_READ_TY_A { ... each variants ... } and and From<fty> for VALUE_READ_TY_A.
                    add_from_variants(mod_items, &variants, &value_read_ty, &fty, &description, rv);

                    // prepare code for each match arm. If we have reserved variant, the match operation would
                    // return an Option, thus we wrap the return value with Some.
                    let mut arms = TokenStream::new();
                    for v in variants.iter().map(|v| {
                        let i = util::unsuffixed_or_bool(v.value, width);
                        let pc = &v.pc;

                        if has_reserved_variant {
                            quote! { #i => Some(#value_read_ty::#pc), }
                        } else {
                            quote! { #i => #value_read_ty::#pc, }
                        }
                    }) {
                        arms.extend(v);
                    }

                    // if we have reserved variant, for all values other than defined we return None.
                    // if svd suggests it only would return defined variants but FieldReader has
                    // other values, it's regarded as unreachable and we enter unreachable! macro.
                    // This situation is rare and only exists if unsafe code casts any illegal value
                    // into a FieldReader structure.
                    if has_reserved_variant {
                        arms.extend(quote! {
                            _ => None,
                        });
                    } else if 1 << width.to_ty_width()? != variants.len() {
                        arms.extend(quote! {
                            _ => unreachable!(),
                        });
                    }

                    // prepare the `variant` function. This function would return field value in
                    // Rust structure; if we have reserved variant we return by Option.
                    if has_reserved_variant {
                        enum_items.extend(quote! {
                            #[doc = "Get enumerated values variant"]
                            #inline
                            pub fn variant(&self) -> Option<#value_read_ty> {
                                match self.bits {
                                    #arms
                                }
                            }
                        });
                    } else {
                        enum_items.extend(quote! {
                        #[doc = "Get enumerated values variant"]
                        #inline
                        pub fn variant(&self) -> #value_read_ty {
                            match self.bits {
                                #arms
                            }
                        }});
                    }

                    // for each variant defined, we generate an `is_variant` function.
                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.nksc;

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
                                *self == #value_read_ty::#pc
                            }
                        });
                    }
                }
            }

            // if this value is derived from a base, generate `pub use` code for each read proxy and value
            // if necessary.
            if let Some((evs, Some(base))) = lookup_filter(&lookup_results, Usage::Read) {
                // preserve value; if read type equals write type, writer would not generate value type again
                evs_r = Some(evs.clone());
                // generate pub use field_1 reader as field_2 reader
                let base_field = util::replace_suffix(base.field, "");
                let base_constant_case = base_field.to_sanitized_constant_case();
                let base_r = Ident::new(&(base_constant_case + "_R"), span);
                derive_from_base(mod_items, &base, &reader_ty, &base_r, &field_reader_brief);
                // only pub use enum when base.register != None. if base.register == None, it emits
                // pub use enum from same module which is not expected
                if base.register != None {
                    // use the same enum structure name
                    derive_from_base(
                        mod_items,
                        &base,
                        &value_read_ty,
                        &value_read_ty,
                        &description,
                    );
                }
            }

            if let Some((first, dim, increment, suffixes, suffixes_str)) = &field_dim {
                let offset_calc = calculate_offset(*first, *increment, offset, true);
                let value = quote! { ((self.bits >> #offset_calc) & #hexmask) #cast };
                let doc = &util::replace_suffix(&description, suffixes_str);
                r_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub unsafe fn #name_snake_case(&self, n: u8) -> #reader_ty {
                        #reader_ty::new ( #value )
                    }
                });
                for (i, suffix) in (0..*dim).zip(suffixes.iter()) {
                    let sub_offset = offset + (i as u64) * (*increment as u64);
                    let value = if sub_offset != 0 {
                        let sub_offset = &util::unsuffixed(sub_offset);
                        quote! {
                            ((self.bits >> #sub_offset) & #hexmask) #cast
                        }
                    } else if use_mask {
                        quote! {
                            (self.bits & #hexmask) #cast
                        }
                    } else {
                        quote! {
                            self.bits
                        }
                    };
                    let name_snake_case_n = Ident::new(
                        &util::replace_suffix(&f.name, suffix).to_sanitized_snake_case(),
                        Span::call_site(),
                    );
                    let doc = util::replace_suffix(
                        &description_with_bits(description_raw, sub_offset, width),
                        suffix,
                    );
                    r_impl_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        pub fn #name_snake_case_n(&self) -> #reader_ty {
                            #reader_ty::new ( #value )
                        }
                    });
                }
            } else {
                let doc = description_with_bits(description_raw, offset, width);
                r_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub fn #name_snake_case(&self) -> #reader_ty {
                        #reader_ty::new ( #value )
                    }
                });
            }

            // generate the enumeration functions prepared before.
            if !enum_items.is_empty() {
                mod_items.extend(quote! {
                    impl #reader_ty {
                        #enum_items
                    }
                });
            }
        }

        // If this field can be written, generate write proxy. Generate write value if it differs from
        // the read value, or else we reuse read value.
        if can_write {
            let mwv = f
                .modified_write_values
                .or(register.modified_write_values)
                .unwrap_or_default();
            // gets a brief of write proxy
            let field_writer_brief = if let Some((_, _, _, _, suffixes_str)) = &field_dim {
                format!(
                    "Fields `{}` writer - {}",
                    util::replace_suffix(&f.name, suffixes_str),
                    description,
                )
            } else {
                format!("Field `{}` writer - {}", f.name, description)
            };

            let value_write_ty =
                if let Some((evs, _)) = lookup_filter(&lookup_results, Usage::Write) {
                    let writer_reader_different_enum = evs_r.as_ref() != Some(evs);
                    let ty_suffix = if writer_reader_different_enum {
                        "AW"
                    } else {
                        "A"
                    };
                    if let Some(enum_name) = &evs.name {
                        let enum_name_constant_case = enum_name.to_sanitized_constant_case();
                        let enum_value_write_ty =
                            Ident::new(&format!("{}_{}", enum_name_constant_case, ty_suffix), span);
                        enum_value_write_ty
                    } else {
                        let derived_field_value_write_ty =
                            Ident::new(&format!("{}_{}", name_constant_case, ty_suffix), span);
                        derived_field_value_write_ty
                    }
                } else {
                    let raw_field_value_write_ty = fty.clone();
                    raw_field_value_write_ty
                };

            // name of write proxy type
            let writer_ty = Ident::new(&(name_constant_case.clone() + "_W"), span);

            let mut proxy_items = TokenStream::new();
            let mut unsafety = unsafety(f.write_constraint.as_ref(), width);

            // if we writes to enumeratedValues, generate its structure if it differs from read structure.
            if let Some((evs, None)) = lookup_filter(&lookup_results, Usage::Write) {
                // parse variants from enumeratedValues svd record
                let variants = Variant::from_enumerated_values(evs, config.pascal_enum_values)?;

                // if the write structure is finite, it can be safely written.
                if variants.len() == 1 << width {
                    unsafety = false;
                }

                // does the read and the write value has the same name? If we have the same,
                // we can reuse read value type other than generating a new one.
                let writer_reader_different_enum = evs_r.as_ref() != Some(evs);

                // generate write value structure and From conversation if we can't reuse read value structure.
                if writer_reader_different_enum {
                    if variants.is_empty() {
                        add_with_no_variants(mod_items, &value_write_ty, &fty, &description, rv);
                    } else {
                        add_from_variants(
                            mod_items,
                            &variants,
                            &value_write_ty,
                            &fty,
                            &description,
                            rv,
                        );
                    }
                }

                // for each variant defined, generate a write function to this field.
                for v in &variants {
                    let pc = &v.pc;
                    let sc = &v.sc;
                    let doc = util::escape_brackets(util::respace(&v.doc).as_ref());
                    proxy_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        pub fn #sc(self) -> &'a mut W {
                            self.variant(#value_write_ty::#pc)
                        }
                    });
                }
            }

            // derive writer. We derive writer if the write proxy is in current register module,
            // or writer in different register have different _SPEC structures
            let should_derive_writer = match lookup_filter(&lookup_results, Usage::Write) {
                Some((_evs, Some(base))) => base.register != None,
                Some((_evs, None)) => true,
                None => true,
            };

            // derive writer structure by type alias to generic write proxy structure.
            if should_derive_writer {
                let proxy = if width == 1 {
                    let wproxy = Ident::new(
                        match mwv {
                            ModifiedWriteValues::Modify => "BitWriter",
                            ModifiedWriteValues::OneToSet | ModifiedWriteValues::Set => {
                                "BitWriter1S"
                            }
                            ModifiedWriteValues::ZeroToClear | ModifiedWriteValues::Clear => {
                                "BitWriter0C"
                            }
                            ModifiedWriteValues::OneToClear => "BitWriter1C",
                            ModifiedWriteValues::ZeroToSet => "BitWriter0C",
                            ModifiedWriteValues::OneToToggle => "BitWriter1T",
                            ModifiedWriteValues::ZeroToToggle => "BitWriter0T",
                        },
                        span,
                    );
                    quote! { crate::#wproxy<'a, #rty, #name_constant_case_spec, #value_write_ty, O> }
                } else {
                    let wproxy = Ident::new(
                        if unsafety {
                            "FieldWriter"
                        } else {
                            "FieldWriterSafe"
                        },
                        span,
                    );
                    let width = &util::unsuffixed(width as _);
                    quote! { crate::#wproxy<'a, #rty, #name_constant_case_spec, #fty, #value_write_ty, #width, O> }
                };
                mod_items.extend(quote! {
                    #[doc = #field_writer_brief]
                    pub type #writer_ty<'a, const O: u8> = #proxy;
                });
            }

            // generate proxy items from collected information
            if !proxy_items.is_empty() {
                mod_items.extend(quote! {
                    impl<'a, const O: u8> #writer_ty<'a, O> {
                        #proxy_items
                    }
                });
            }

            if let Some((evs, Some(base))) = lookup_filter(&lookup_results, Usage::Write) {
                // if base.register == None, it emits pub use structure from same module.
                if base.register != None {
                    let writer_reader_different_enum = evs_r.as_ref() != Some(evs);
                    if writer_reader_different_enum {
                        // use the same enum structure name
                        derive_from_base(
                            mod_items,
                            &base,
                            &value_write_ty,
                            &value_write_ty,
                            &description,
                        );
                    }
                } else {
                    // if base.register == None, derive write from the same module. This is allowed because both
                    // the generated and source write proxy are in the same module.
                    // we never reuse writer for writer in different module does not have the same _SPEC strcuture,
                    // thus we cannot write to current register using re-exported write proxy.

                    // generate pub use field_1 writer as field_2 writer
                    let base_field = util::replace_suffix(base.field, "");
                    let base_constant_case = base_field.to_sanitized_constant_case();
                    let base_w = Ident::new(&(base_constant_case + "_W"), span);
                    derive_from_base(mod_items, &base, &writer_ty, &base_w, &field_writer_brief);
                }
            }

            if let Some((_, dim, increment, suffixes, suffixes_str)) = &field_dim {
                let doc = &util::replace_suffix(&description, suffixes_str);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub unsafe fn #name_snake_case<const O: u8>(&mut self) -> #writer_ty<O> {
                        #writer_ty::new(self)
                    }
                });

                for (i, suffix) in (0..*dim).zip(suffixes.iter()) {
                    let sub_offset = offset + (i as u64) * (*increment as u64);
                    let name_snake_case_n = Ident::new(
                        &util::replace_suffix(&f.name, suffix).to_sanitized_snake_case(),
                        Span::call_site(),
                    );
                    let doc = util::replace_suffix(
                        &description_with_bits(description_raw, sub_offset, width),
                        suffix,
                    );
                    let sub_offset = util::unsuffixed(sub_offset as u64);

                    w_impl_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        pub fn #name_snake_case_n(&mut self) -> #writer_ty<#sub_offset> {
                            #writer_ty::new(self)
                        }
                    });
                }
            } else {
                let doc = description_with_bits(description_raw, offset, width);
                let offset = util::unsuffixed(offset as u64);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    pub fn #name_snake_case(&mut self) -> #writer_ty<#offset> {
                        #writer_ty::new(self)
                    }
                });
            }
        }
    }

    Ok(())
}

fn unsafety(write_constraint: Option<&WriteConstraint>, width: u32) -> bool {
    match &write_constraint {
        Some(&WriteConstraint::Range(range))
            if range.min == 0 && range.max == u64::MAX >> (64 - width) =>
        {
            // the SVD has acknowledged that it's safe to write
            // any value that can fit in the field
            false
        }
        None if width == 1 => {
            // the field is one bit wide, so we assume it's legal to write
            // either value into it or it wouldn't exist; despite that
            // if a writeConstraint exists then respect it
            false
        }
        _ => true,
    }
}

struct Variant {
    doc: String,
    pc: Ident,
    nksc: Ident,
    sc: Ident,
    value: u64,
}

impl Variant {
    fn from_enumerated_values(evs: &EnumeratedValues, pc: bool) -> Result<Vec<Self>> {
        let span = Span::call_site();
        evs.values
            .iter()
            // filter out all reserved variants, as we should not
            // generate code for them
            .filter(|field| field.name.to_lowercase() != "reserved" && field.is_default == None)
            .map(|ev| {
                let value = u64(ev.value.ok_or_else(|| {
                    anyhow!("EnumeratedValue {} has no `<value>` field", ev.name)
                })?);

                let nksc = ev.name.to_sanitized_not_keyword_snake_case();
                let sc = util::sanitize_keyword(nksc.clone());
                Ok(Variant {
                    doc: ev
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("`{:b}`", value)),
                    pc: Ident::new(
                        &(if pc {
                            ev.name.to_sanitized_pascal_case()
                        } else {
                            ev.name.to_sanitized_constant_case()
                        }),
                        span,
                    ),
                    nksc: Ident::new(&nksc, span),
                    sc: Ident::new(&sc, span),
                    value,
                })
            })
            .collect::<Result<Vec<_>>>()
    }
}

fn add_with_no_variants(
    mod_items: &mut TokenStream,
    pc: &Ident,
    fty: &Ident,
    desc: &str,
    reset_value: Option<u64>,
) {
    let cast = if fty == "bool" {
        quote! { val.0 as u8 != 0 }
    } else {
        quote! { val.0 as _ }
    };

    let desc = if let Some(rv) = reset_value {
        format!("{}\n\nValue on reset: {}", desc, rv)
    } else {
        desc.to_owned()
    };

    mod_items.extend(quote! {
        #[doc = #desc]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct #pc(#fty);
        impl From<#pc> for #fty {
            #[inline(always)]
            fn from(val: #pc) -> Self {
                #cast
            }
        }
    });
}

fn add_from_variants(
    mod_items: &mut TokenStream,
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
        let desc = util::escape_brackets(&util::respace(&format!("{}: {}", v.value, v.doc)));
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

    mod_items.extend(quote! {
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

fn calculate_offset(
    first: u32,
    increment: u32,
    offset: u64,
    with_parentheses: bool,
) -> TokenStream {
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
    let single_ident = (first == 0) && (increment == 1) && (offset == 0);
    if with_parentheses && !single_ident {
        quote! { (#res) }
    } else {
        res
    }
}

fn description_with_bits(description: &str, offset: u64, width: u32) -> String {
    let mut res = if width == 1 {
        format!("Bit {}", offset)
    } else {
        format!("Bits {}:{}", offset, offset + width as u64 - 1)
    };
    if !description.is_empty() {
        res.push_str(" - ");
        res.push_str(&util::respace(&util::escape_brackets(description)));
    }
    res
}

fn derive_from_base(
    mod_items: &mut TokenStream,
    base: &Base,
    pc: &Ident,
    base_pc: &Ident,
    desc: &str,
) {
    let span = Span::call_site();
    let path = if let (Some(peripheral), Some(register)) = (&base.peripheral, &base.register) {
        let pmod_ = peripheral.to_sanitized_snake_case();
        let rmod_ = register.to_sanitized_snake_case();
        let pmod_ = Ident::new(&pmod_, span);
        let rmod_ = Ident::new(&rmod_, span);

        quote! { crate::#pmod_::#rmod_::#base_pc }
    } else if let Some(register) = &base.register {
        let mod_ = register.to_sanitized_snake_case();
        let mod_ = Ident::new(&mod_, span);

        quote! { super::#mod_::#base_pc }
    } else {
        quote! { #base_pc }
    };
    mod_items.extend(quote! {
        #[doc = #desc]
        pub use #path as #pc;
    });
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Base<'a> {
    pub peripheral: Option<&'a str>,
    pub register: Option<&'a str>,
    pub field: &'a str,
}

impl<'a> Base<'a> {
    pub fn from_field(field: &'a str) -> Self {
        Self {
            peripheral: None,
            register: None,
            field,
        }
    }
}

fn lookup<'a>(
    evs: &'a [EnumeratedValues],
    fields: &'a [&'a Field],
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
            return Some((*evs, *base));
        }
    }

    evs.first().cloned()
}

fn lookup_in_fields<'f>(
    base_evs: &str,
    base_field: &str,
    fields: &'f [&'f Field],
    register: &Register,
) -> Result<(&'f EnumeratedValues, Option<Base<'f>>)> {
    if let Some(base_field) = fields.iter().find(|f| f.name == base_field) {
        lookup_in_field(base_evs, None, None, base_field)
    } else {
        Err(anyhow!(
            "Field {} not found in register {}",
            base_field,
            register.name
        ))
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
        if let Some(field) = register.get_field(base_field) {
            lookup_in_field(base_evs, Some(base_register), base_peripheral, field)
        } else {
            Err(anyhow!(
                "No field {} in register {}",
                base_field,
                register.name
            ))
        }
    } else {
        Err(anyhow!(
            "No register {} in peripheral {}",
            base_register,
            peripheral.name
        ))
    }
}

fn lookup_in_field<'f>(
    base_evs: &str,
    base_register: Option<&'f str>,
    base_peripheral: Option<&'f str>,
    field: &'f Field,
) -> Result<(&'f EnumeratedValues, Option<Base<'f>>)> {
    for evs in &field.enumerated_values {
        if evs.name.as_deref() == Some(base_evs) {
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

    Err(anyhow!(
        "No EnumeratedValues {} in field {}",
        base_evs,
        field.name
    ))
}

fn lookup_in_register<'r>(
    base_evs: &str,
    register: &'r Register,
) -> Result<(&'r EnumeratedValues, Option<Base<'r>>)> {
    let mut matches = vec![];

    for f in register.fields() {
        if let Some(evs) = f
            .enumerated_values
            .iter()
            .find(|evs| evs.name.as_deref() == Some(base_evs))
        {
            matches.push((evs, &f.name))
        }
    }

    match &matches[..] {
        [] => Err(anyhow!(
            "EnumeratedValues {} not found in register {}",
            base_evs,
            register.name
        )),
        [(evs, field)] => Ok((evs, Some(Base::from_field(field)))),
        matches => {
            let fields = matches.iter().map(|(f, _)| &f.name).collect::<Vec<_>>();
            Err(anyhow!(
                "Fields {:?} have an enumeratedValues named {}",
                fields,
                base_evs
            ))
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
        let all_registers = peripheral.all_registers().collect::<Vec<_>>();
        lookup_in_peripheral(
            Some(base_peripheral),
            base_register,
            base_field,
            base_evs,
            all_registers.as_slice(),
            peripheral,
        )
    } else {
        Err(anyhow!("No peripheral {}", base_peripheral))
    }
}
