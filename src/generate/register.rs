use crate::svd::{
    Access, BitRange, EnumeratedValues, Field, MaybeArray, ModifiedWriteValues, ReadAction,
    Register, RegisterProperties, Usage, WriteConstraint,
};
use core::u64;
use log::warn;
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};
use quote::{quote, ToTokens};
use std::borrow::Cow;
use std::collections::HashSet;
use svd_parser::expand::{
    derive_enumerated_values, derive_field, BlockPath, EnumPath, FieldPath, Index, RegisterPath,
};

use crate::util::{
    self, ident_to_path, path_segment, replace_suffix, type_path, Config, FullName,
    ToSanitizedCase, U32Ext,
};
use anyhow::{anyhow, Result};
use syn::punctuated::Punctuated;

pub fn render(
    register: &Register,
    path: &BlockPath,
    dpath: Option<RegisterPath>,
    index: &Index,
    config: &Config,
) -> Result<TokenStream> {
    let mut name = util::name_of(register, config.ignore_groups);
    // Rename if this is a derived array
    if dpath.is_some() {
        if let MaybeArray::Array(info, array_info) = register {
            if let Some(dim_index) = &array_info.dim_index {
                let index: Cow<str> = dim_index.first().unwrap().into();
                name = replace_suffix(&info.fullname(config.ignore_groups), &index).into()
            }
        }
    }
    let span = Span::call_site();
    let name_constant_case = name.to_constant_case_ident(span);
    let name_snake_case = name.to_snake_case_ident(span);
    let description = util::escape_special_chars(
        util::respace(&register.description.clone().unwrap_or_else(|| {
            warn!("Missing description for register {}", register.name);
            Default::default()
        }))
        .as_ref(),
    );

    if let Some(dpath) = dpath.as_ref() {
        let mut derived = if &dpath.block == path {
            type_path(Punctuated::new())
        } else {
            util::block_path_to_ty(&dpath.block, span)
        };
        let dname = util::name_of(index.registers.get(dpath).unwrap(), config.ignore_groups);
        let mut mod_derived = derived.clone();
        derived
            .path
            .segments
            .push(path_segment(dname.to_constant_case_ident(span)));
        mod_derived
            .path
            .segments
            .push(path_segment(dname.to_snake_case_ident(span)));

        Ok(quote! {
            pub use #derived as #name_constant_case;
            pub use #mod_derived as #name_snake_case;
        })
    } else {
        let regspec_ident = format!("{name}_SPEC").to_constant_case_ident(span);
        let access = util::access_of(&register.properties, register.fields.as_deref());
        let accs = if access.can_read() && access.can_write() {
            "rw"
        } else if access.can_write() {
            "w"
        } else if access.can_read() {
            "r"
        } else {
            return Err(anyhow!("Incorrect access of register {}", register.name));
        };
        let alias_doc =
            format!("{name} ({accs}) register accessor: an alias for `Reg<{regspec_ident}>`");
        let mut out = TokenStream::new();
        out.extend(quote! {
            #[doc = #alias_doc]
            pub type #name_constant_case = crate::Reg<#name_snake_case::#regspec_ident>;
        });
        let mod_items = render_register_mod(
            register,
            access,
            &path.new_register(&register.name),
            index,
            config,
        )?;

        out.extend(quote! {
            #[doc = #description]
            pub mod #name_snake_case {
                #mod_items
            }
        });

        Ok(out)
    }
}

pub fn render_register_mod(
    register: &Register,
    access: Access,
    path: &RegisterPath,
    index: &Index,
    config: &Config,
) -> Result<TokenStream> {
    let properties = &register.properties;
    let name = util::name_of(register, config.ignore_groups);
    let span = Span::call_site();
    let regspec_ident = format!("{name}_SPEC").to_constant_case_ident(span);
    let name_snake_case = name.to_snake_case_ident(span);
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
    let description = util::escape_special_chars(
        util::respace(&register.description.clone().unwrap_or_else(|| {
            warn!("Missing description for register {}", register.name);
            Default::default()
        }))
        .as_ref(),
    );

    let mut mod_items = TokenStream::new();
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
            pub struct R(crate::R<#regspec_ident>);
        });

        if !config.derive_more {
            mod_items.extend(quote! {
                impl core::ops::Deref for R {
                    type Target = crate::R<#regspec_ident>;

                    #[inline(always)]
                    fn deref(&self) -> &Self::Target {
                        &self.0
                    }
                }

                impl From<crate::R<#regspec_ident>> for R {
                    #[inline(always)]
                    fn from(reader: crate::R<#regspec_ident>) -> Self {
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
            pub struct W(crate::W<#regspec_ident>);
        });

        if !config.derive_more {
            mod_items.extend(quote! {
                impl core::ops::Deref for W {
                    type Target = crate::W<#regspec_ident>;

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

                impl From<crate::W<#regspec_ident>> for W {
                    #[inline(always)]
                    fn from(writer: crate::W<#regspec_ident>) -> Self {
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

    let mut r_impl_items = TokenStream::new();
    let mut r_debug_impl = TokenStream::new();
    let mut w_impl_items = TokenStream::new();
    let mut zero_to_modify_fields_bitmap = 0;
    let mut one_to_modify_fields_bitmap = 0;

    let open = Punct::new('{', Spacing::Alone);
    let close = Punct::new('}', Spacing::Alone);

    if let Some(cur_fields) = register.fields.as_ref() {
        // filter out all reserved fields, as we should not generate code for
        // them
        let cur_fields: Vec<&Field> = cur_fields
            .iter()
            .filter(|field| field.name.to_lowercase() != "reserved")
            .collect();

        if !cur_fields.is_empty() {
            if config.impl_debug {
                r_debug_impl.extend(render_register_mod_debug(
                    register,
                    &access,
                    &cur_fields,
                    config,
                ))
            }

            (
                r_impl_items,
                w_impl_items,
                zero_to_modify_fields_bitmap,
                one_to_modify_fields_bitmap,
            ) = fields(
                cur_fields,
                &regspec_ident,
                register.modified_write_values,
                access,
                properties,
                &mut mod_items,
                path,
                index,
                config,
            )?;
        }
    } else if !access.can_read() || register.read_action.is_some() {
        if let Some(feature) = &config.impl_debug_feature {
            r_debug_impl.extend(quote! {
                #[cfg(feature=#feature)]
            });
        }
        r_debug_impl.extend(quote! {
            impl core::fmt::Debug for crate::generic::Reg<#regspec_ident> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "(not readable)")
                }
            }
        });
    } else {
        // no register fields are defined so implement Debug to get entire register value
        if let Some(feature) = &config.impl_debug_feature {
            r_debug_impl.extend(quote! {
                #[cfg(feature=#feature)]
            });
        }
        r_debug_impl.extend(quote! {
            impl core::fmt::Debug for R {
                fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                    write!(f, "{}", self.bits())
                }
            }
        });
        if let Some(feature) = &config.impl_debug_feature {
            r_debug_impl.extend(quote! {
                #[cfg(feature=#feature)]
            });
        }
        r_debug_impl.extend(quote! {
            impl core::fmt::Debug for crate::generic::Reg<#regspec_ident> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    self.read().fmt(f)
                }
            }
        });
    }

    if can_read && !r_impl_items.is_empty() {
        mod_items.extend(quote! {
            impl R #open #r_impl_items #close
        });
    }
    if !r_debug_impl.is_empty() {
        mod_items.extend(quote! {
            #r_debug_impl
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
        // * the write constraints of the register allow full range of values
        let can_write_safe = !unsafety(
            register
                .fields
                .as_ref()
                .and_then(|fields| fields.first())
                .and_then(|field| field.write_constraint)
                .as_ref(),
            rsize,
        ) || !unsafety(register.write_constraint.as_ref(), rsize);

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

    let methods = methods
        .iter()
        .map(|s| format!("[`{s}`](crate::generic::Reg::{s})"))
        .collect::<Vec<_>>();
    let mut doc = format!("{description}\n\nThis register you can {}. See [API](https://docs.rs/svd2rust/#read--modify--write-api).", methods.join(", "));

    if name_snake_case != "cfg" {
        doc += format!(
            "\n\nFor information about available fields see [{name_snake_case}](index.html) module"
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

    mod_items.extend(quote! {
        #[doc = #doc]
        pub struct #regspec_ident;

        impl crate::RegisterSpec for #regspec_ident {
            type Ux = #rty;
        }
    });

    if can_read {
        let doc = format!("`read()` method returns [{name_snake_case}::R](R) reader structure",);
        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Readable for #regspec_ident {
                type Reader = R;
            }
        });
    }
    if can_write {
        let doc =
            format!("`write(|w| ..)` method takes [{name_snake_case}::W](W) writer structure",);

        let zero_to_modify_fields_bitmap = util::hex(zero_to_modify_fields_bitmap);
        let one_to_modify_fields_bitmap = util::hex(one_to_modify_fields_bitmap);

        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Writable for #regspec_ident {
                type Writer = W;
                const ZERO_TO_MODIFY_FIELDS_BITMAP: Self::Ux = #zero_to_modify_fields_bitmap;
                const ONE_TO_MODIFY_FIELDS_BITMAP: Self::Ux = #one_to_modify_fields_bitmap;
            }
        });
    }
    if let Some(rv) = properties.reset_value.map(util::hex) {
        let doc = format!("`reset()` method sets {} to value {rv}", register.name);
        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Resettable for #regspec_ident {
                const RESET_VALUE: Self::Ux = #rv;
            }
        });
    }
    Ok(mod_items)
}

fn render_register_mod_debug(
    register: &Register,
    access: &Access,
    cur_fields: &[&Field],
    config: &Config,
) -> Result<TokenStream> {
    let name = util::name_of(register, config.ignore_groups);
    let span = Span::call_site();
    let regspec_ident = format!("{name}_SPEC").to_constant_case_ident(span);
    let open = Punct::new('{', Spacing::Alone);
    let close = Punct::new('}', Spacing::Alone);
    let mut r_debug_impl = TokenStream::new();

    // implement Debug for register readable fields that have no read side effects
    if access.can_read() && register.read_action.is_none() {
        if let Some(feature) = &config.impl_debug_feature {
            r_debug_impl.extend(quote! {
                #[cfg(feature=#feature)]
            });
        }
        r_debug_impl.extend(quote! {
            impl core::fmt::Debug for R #open
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result #open
                f.debug_struct(#name)
        });
        for &f in cur_fields.iter() {
            let field_access = match &f.access {
                Some(a) => a,
                None => access,
            };
            let bit_or_bits = if f.bit_width() > 1 { "bits" } else { "bit" };
            let bit_or_bits = syn::Ident::new(bit_or_bits, span);
            log::debug!("register={} field={}", name, f.name);
            if field_access.can_read() && f.read_action.is_none() {
                if let Field::Array(_, de) = &f {
                    for (_, suffix) in de.indexes().enumerate() {
                        let f_name_n = util::replace_suffix(&f.name, &suffix)
                            .to_snake_case_ident(Span::call_site());
                        let f_name_n_s = format!("{f_name_n}");
                        r_debug_impl.extend(quote! {
                            .field(#f_name_n_s, &format_args!("{}", self.#f_name_n().#bit_or_bits()))
                        });
                    }
                } else {
                    let f_name = util::replace_suffix(&f.name, "");
                    let f_name = f_name.to_snake_case_ident(span);
                    let f_name_s = format!("{f_name}");
                    r_debug_impl.extend(quote! {
                        .field(#f_name_s, &format_args!("{}", self.#f_name().#bit_or_bits()))
                    });
                }
            }
        }
        r_debug_impl.extend(quote! {
                    .finish()
                #close
            #close
        });
        if let Some(feature) = &config.impl_debug_feature {
            r_debug_impl.extend(quote! {
                #[cfg(feature=#feature)]
            });
        }
        r_debug_impl.extend(quote! {
            impl core::fmt::Debug for crate::generic::Reg<#regspec_ident> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    self.read().fmt(f)
                }
            }
        });
    } else if !access.can_read() || register.read_action.is_some() {
        if let Some(feature) = &config.impl_debug_feature {
            r_debug_impl.extend(quote! {
                #[cfg(feature=#feature)]
            });
        }
        r_debug_impl.extend(quote! {
            impl core::fmt::Debug for crate::generic::Reg<#regspec_ident> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "(not readable)")
                }
            }
        });
    } else {
        warn!("not implementing debug for {name}");
    }
    Ok(r_debug_impl)
}

#[allow(clippy::too_many_arguments)]
pub fn fields(
    mut fields: Vec<&Field>,
    regspec_ident: &Ident,
    rmwv: Option<ModifiedWriteValues>,
    access: Access,
    properties: &RegisterProperties,
    mod_items: &mut TokenStream,
    rpath: &RegisterPath,
    index: &Index,
    config: &Config,
) -> Result<(TokenStream, TokenStream, u64, u64)> {
    let mut r_impl_items = TokenStream::new();
    let mut w_impl_items = TokenStream::new();
    let mut zero_to_modify_fields_bitmap = 0u64;
    let mut one_to_modify_fields_bitmap = 0u64;
    let span = Span::call_site();
    let can_read = access.can_read();
    let can_write = access.can_write();

    fields.sort_by_key(|f| f.bit_offset());

    // Hack for #625
    let mut enum_derives = HashSet::new();
    let mut reader_derives = HashSet::new();
    let mut writer_enum_derives = HashSet::new();
    let mut writer_derives = HashSet::new();

    // TODO enumeratedValues
    let inline = quote! { #[inline(always)] };
    for &f in fields.iter() {
        let mut f = f.clone();
        let mut fpath = None;
        let dpath = f.derived_from.take();
        if let Some(dpath) = dpath {
            fpath = derive_field(&mut f, &dpath, rpath, index)?;
        }
        let fpath = fpath.unwrap_or_else(|| rpath.new_field(&f.name));
        // TODO(AJM) - do we need to do anything with this range type?
        let BitRange { offset, width, .. } = f.bit_range;

        if f.is_single() && f.name.contains("%s") {
            return Err(anyhow!("incorrect field {}", f.name));
        }

        let name = util::replace_suffix(&f.name, "");
        let name_snake_case = name.to_snake_case_ident(span);
        let name_constant_case = name.to_sanitized_constant_case();
        let description_raw = f.description.as_deref().unwrap_or(""); // raw description, if absent using empty string
        let description = util::respace(&util::escape_special_chars(description_raw));

        let can_read = can_read
            && (f.access != Some(Access::WriteOnly))
            && (f.access != Some(Access::WriteOnce));
        let can_write = can_write && (f.access != Some(Access::ReadOnly));

        let mask = u64::MAX >> (64 - width);
        let hexmask = &util::digit_or_hex(mask);
        let offset = u64::from(offset);
        let rv = properties.reset_value.map(|rv| (rv >> offset) & mask);
        let fty = width.to_ty()?;

        let use_mask;
        let use_cast;
        if let Some(size) = properties.size {
            let size = size.to_ty_width()?;
            use_cast = size != width.to_ty_width()?;
            use_mask = size != width;
        } else {
            use_cast = true;
            use_mask = true;
        };

        let mut lookup_results = Vec::new();
        for mut ev in f.enumerated_values.clone().into_iter() {
            let mut epath = None;
            let dpath = ev.derived_from.take();
            if let Some(dpath) = dpath {
                epath = Some(derive_enumerated_values(&mut ev, &dpath, &fpath, index)?);
            }
            // TODO: remove this hack
            if let Some(epath) = epath.as_ref() {
                ev = (*index.evs.get(epath).unwrap()).clone();
            }
            lookup_results.push((ev, epath));
        }

        let mut evs_r = None;

        let brief_suffix = if let Field::Array(_, de) = &f {
            if let Some(range) = de.indexes_as_range() {
                format!("[{}-{}]", *range.start(), *range.end())
            } else {
                let suffixes: Vec<_> = de.indexes().collect();
                format!("[{}]", suffixes.join(","))
            }
        } else {
            String::new()
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
                quote! { (self.bits >> #offset) }
            } else {
                quote! { self.bits }
            };
            let value = if use_mask && use_cast {
                quote! { (#value & #hexmask) #cast }
            } else if use_mask {
                quote! { #value & #hexmask }
            } else {
                value
            };

            // get a brief description for this field
            // the suffix string from field name is removed in brief description.
            let field_reader_brief = format!("Field `{name}{brief_suffix}` reader - {description}");

            // get the type of value structure. It can be generated from either name field
            // in enumeratedValues if it's an enumeration, or from field name directly if it's not.
            let value_read_ty = if let Some((evs, _)) = lookup_filter(&lookup_results, Usage::Read)
            {
                if let Some(enum_name) = &evs.name {
                    format!("{enum_name}_A").to_constant_case_ident(span)
                } else {
                    // derived_field_value_read_ty
                    Ident::new(&format!("{name_constant_case}_A"), span)
                }
            } else {
                // raw_field_value_read_ty
                fty.clone()
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
                    if value_read_ty == "bool" {
                        quote! { crate::BitReader }
                    } else {
                        quote! { crate::BitReader<#value_read_ty> }
                    }
                } else if value_read_ty == "u8" {
                    quote! { crate::FieldReader }
                } else {
                    quote! { crate::FieldReader<#value_read_ty> }
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
                evs_r = Some(evs);

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
                                format!("is{sc}")
                            } else {
                                format!("is_{sc}")
                            },
                            span,
                        );

                        let doc = format!("Checks if the value of the field is `{pc}`");
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
                evs_r = Some(evs);
                // generate pub use field_1 reader as field_2 reader
                let base_field = util::replace_suffix(&base.field.name, "");
                let base_r = (base_field + "_R").to_constant_case_ident(span);
                if !reader_derives.contains(&reader_ty) {
                    let base_path = base_syn_path(base, &fpath, &base_r)?;
                    mod_items.extend(quote! {
                        #[doc = #field_reader_brief]
                        pub use #base_path as #reader_ty;
                    });
                    reader_derives.insert(reader_ty.clone());
                }
                // only pub use enum when base.register != None. if base.register == None, it emits
                // pub use enum from same module which is not expected
                if base.register() != fpath.register() {
                    // use the same enum structure name
                    if !enum_derives.contains(&value_read_ty) {
                        let base_path = base_syn_path(base, &fpath, &value_read_ty)?;
                        mod_items.extend(quote! {
                            #[doc = #description]
                            pub use #base_path as #value_read_ty;
                        });
                        enum_derives.insert(value_read_ty.clone());
                    }
                }
            }

            if let Field::Array(_, de) = &f {
                let increment = de.dim_increment;
                let doc = &util::replace_suffix(&description, &brief_suffix);
                if let Some(range) = de.indexes_as_range() {
                    let first = *range.start();

                    let offset_calc = calculate_offset(first, increment, offset, true);
                    let value = quote! { ((self.bits >> #offset_calc) & #hexmask) #cast };
                    r_impl_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        pub unsafe fn #name_snake_case(&self, n: u8) -> #reader_ty {
                            #reader_ty::new ( #value )
                        }
                    });
                }
                for (i, suffix) in de.indexes().enumerate() {
                    let sub_offset = offset + (i as u64) * (increment as u64);
                    let value = if sub_offset != 0 {
                        let sub_offset = &util::unsuffixed(sub_offset);
                        quote! { (self.bits >> #sub_offset) }
                    } else {
                        quote! { self.bits }
                    };
                    let value = if use_mask && use_cast {
                        quote! { (#value & #hexmask) #cast }
                    } else if use_mask {
                        quote! { #value & #hexmask }
                    } else {
                        value
                    };
                    let name_snake_case_n = util::replace_suffix(&f.name, &suffix)
                        .to_snake_case_ident(Span::call_site());
                    let doc = util::replace_suffix(
                        &description_with_bits(description_raw, sub_offset, width),
                        &suffix,
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
            let mwv = f.modified_write_values.or(rmwv).unwrap_or_default();
            // gets a brief of write proxy
            let field_writer_brief = format!("Field `{name}{brief_suffix}` writer - {description}");

            let value_write_ty =
                if let Some((evs, _)) = lookup_filter(&lookup_results, Usage::Write) {
                    let writer_reader_different_enum = evs_r != Some(evs);
                    let ty_suffix = if writer_reader_different_enum {
                        "AW"
                    } else {
                        "A"
                    };
                    if let Some(enum_name) = &evs.name {
                        format!("{enum_name}_{ty_suffix}").to_constant_case_ident(span)
                    } else {
                        // derived_field_value_write_ty
                        Ident::new(&format!("{name_constant_case}_{ty_suffix}"), span)
                    }
                } else {
                    // raw_field_value_write_ty
                    fty.clone()
                };

            // name of write proxy type
            let writer_ty = Ident::new(&(name_constant_case.clone() + "_W"), span);

            let mut proxy_items = TokenStream::new();
            let mut unsafety = unsafety(f.write_constraint.as_ref(), width);

            // if we writes to enumeratedValues, generate its structure if it differs from read structure.
            if let Some((evs, epath)) = lookup_filter(&lookup_results, Usage::Write) {
                if epath.as_ref().filter(|base| base.register() == fpath.register() || writer_enum_derives.contains(&value_write_ty)).is_none() {
                    // parse variants from enumeratedValues svd record
                    let variants = Variant::from_enumerated_values(evs, config.pascal_enum_values)?;

                    // if the write structure is finite, it can be safely written.
                    if variants.len() == 1 << width {
                        unsafety = false;
                    }

                    if epath.is_none() {
                        // does the read and the write value has the same name? If we have the same,
                        // we can reuse read value type other than generating a new one.
                        let writer_reader_different_enum = evs_r != Some(evs);
    
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
                    }

                    // for each variant defined, generate a write function to this field.
                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.sc;
                        let doc = util::escape_special_chars(&util::respace(&v.doc));
                        proxy_items.extend(quote! {
                            #[doc = #doc]
                            #inline
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#value_write_ty::#pc)
                            }
                        });
                    }
                }
            }

            // derive writer. We derive writer if the write proxy is in current register module,
            // or writer in different register have different _SPEC structures
            let should_derive_writer = match lookup_filter(&lookup_results, Usage::Write) {
                Some((_evs, Some(base))) => base.register() != fpath.register(),
                Some((_evs, None)) => true,
                None => true,
            };

            // derive writer structure by type alias to generic write proxy structure.
            if should_derive_writer {
                let proxy = if width == 1 {
                    use ModifiedWriteValues::*;
                    let wproxy = Ident::new(
                        match mwv {
                            Modify | Set | Clear => "BitWriter",
                            OneToSet => "BitWriter1S",
                            ZeroToClear => "BitWriter0C",
                            OneToClear => "BitWriter1C",
                            ZeroToSet => "BitWriter0C",
                            OneToToggle => "BitWriter1T",
                            ZeroToToggle => "BitWriter0T",
                        },
                        span,
                    );
                    if value_write_ty == "bool" {
                        quote! { crate::#wproxy<'a, #regspec_ident, O> }
                    } else {
                        quote! { crate::#wproxy<'a, #regspec_ident, O, #value_write_ty> }
                    }
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
                    if value_write_ty == "u8" {
                        quote! { crate::#wproxy<'a, #regspec_ident, #width, O> }
                    } else {
                        quote! { crate::#wproxy<'a, #regspec_ident, #width, O, #value_write_ty> }
                    }
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
                if base.register() != fpath.register() {
                    let writer_reader_different_enum = evs_r != Some(evs);
                    if writer_reader_different_enum {
                        // use the same enum structure name
                        if !writer_enum_derives.contains(&value_write_ty) {
                            let base_path = base_syn_path(base, &fpath, &value_write_ty)?;
                            mod_items.extend(quote! {
                                #[doc = #description]
                                pub use #base_path as #value_write_ty;
                            });
                            writer_enum_derives.insert(value_write_ty.clone());
                        }
                    }
                } else {
                    // if base.register == None, derive write from the same module. This is allowed because both
                    // the generated and source write proxy are in the same module.
                    // we never reuse writer for writer in different module does not have the same _SPEC strcuture,
                    // thus we cannot write to current register using re-exported write proxy.

                    // generate pub use field_1 writer as field_2 writer
                    let base_field = util::replace_suffix(&base.field.name, "");
                    let base_w = (base_field + "_W").to_constant_case_ident(span);
                    if !writer_derives.contains(&writer_ty) {
                        let base_path = base_syn_path(base, &fpath, &base_w)?;
                        mod_items.extend(quote! {
                            #[doc = #field_writer_brief]
                            pub use #base_path as #writer_ty;
                        });
                        writer_derives.insert(writer_ty.clone());
                    }
                }
            }

            // Generate field writer accessors
            if let Field::Array(_, de) = &f {
                let increment = de.dim_increment;
                let doc = &util::replace_suffix(&description, &brief_suffix);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    #[must_use]
                    pub unsafe fn #name_snake_case<const O: u8>(&mut self) -> #writer_ty<O> {
                        #writer_ty::new(self)
                    }
                });

                for (i, suffix) in de.indexes().enumerate() {
                    let sub_offset = offset + (i as u64) * (increment as u64);
                    let name_snake_case_n = &util::replace_suffix(&f.name, &suffix)
                        .to_snake_case_ident(Span::call_site());
                    let doc = util::replace_suffix(
                        &description_with_bits(description_raw, sub_offset, width),
                        &suffix,
                    );
                    let sub_offset = util::unsuffixed(sub_offset);

                    w_impl_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        #[must_use]
                        pub fn #name_snake_case_n(&mut self) -> #writer_ty<#sub_offset> {
                            #writer_ty::new(self)
                        }
                    });
                }
            } else {
                let doc = description_with_bits(description_raw, offset, width);
                let offset = util::unsuffixed(offset);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    #[must_use]
                    pub fn #name_snake_case(&mut self) -> #writer_ty<#offset> {
                        #writer_ty::new(self)
                    }
                });
            }
            let bitmask = (u64::MAX >> (64 - width)) << offset;
            use ModifiedWriteValues::*;
            match mwv {
                Modify | Set | Clear => {}
                OneToSet | OneToClear | OneToToggle => {
                    one_to_modify_fields_bitmap |= bitmask;
                }
                ZeroToClear | ZeroToSet | ZeroToToggle => {
                    zero_to_modify_fields_bitmap |= bitmask;
                }
            }
        }
    }

    Ok((
        r_impl_items,
        w_impl_items,
        zero_to_modify_fields_bitmap,
        one_to_modify_fields_bitmap,
    ))
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
            .filter(|field| field.name.to_lowercase() != "reserved" && field.is_default.is_none())
            .map(|ev| {
                let value = ev
                    .value
                    .ok_or_else(|| anyhow!("EnumeratedValue {} has no `<value>` field", ev.name))?;

                let nksc = ev.name.to_sanitized_not_keyword_snake_case();
                let sc = util::sanitize_keyword(nksc.clone());
                Ok(Variant {
                    doc: ev
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("`{value:b}`")),
                    pc: if pc {
                        ev.name.to_pascal_case_ident(span)
                    } else {
                        ev.name.to_constant_case_ident(span)
                    },
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
        format!("{desc}\n\nValue on reset: {rv}")
    } else {
        desc.to_string()
    };

    mod_items.extend(quote! {
        #[doc = #desc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct #pc(#fty);
        impl From<#pc> for #fty {
            #[inline(always)]
            fn from(val: #pc) -> Self {
                #cast
            }
        }
    });
    if fty != "bool" {
        mod_items.extend(quote! {
            impl crate::FieldSpec for #pc {
                type Ux = #fty;
            }
        });
    }
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
        let desc = util::escape_special_chars(&util::respace(&format!("{}: {}", v.value, v.doc)));
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
        format!("{desc}\n\nValue on reset: {rv}")
    } else {
        desc.to_string()
    };

    mod_items.extend(quote! {
        #[doc = #desc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    if fty != "bool" {
        mod_items.extend(quote! {
            impl crate::FieldSpec for #pc {
                type Ux = #fty;
            }
        });
    }
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
        format!("Bit {offset}")
    } else {
        format!("Bits {offset}:{}", offset + width as u64 - 1)
    };
    if !description.is_empty() {
        res.push_str(" - ");
        res.push_str(&util::respace(&util::escape_special_chars(description)));
    }
    res
}

fn base_syn_path(
    base: &EnumPath,
    fpath: &FieldPath,
    base_ident: &Ident,
) -> Result<syn::TypePath, syn::Error> {
    let span = Span::call_site();
    let path = if base.register() == fpath.register() {
        ident_to_path(base_ident.clone())
    } else if base.register().block == fpath.register().block {
        let mut segments = Punctuated::new();
        segments.push(path_segment(Ident::new("super", span)));
        segments.push(path_segment(base.register().name.to_snake_case_ident(span)));
        segments.push(path_segment(base_ident.clone()));
        type_path(segments)
    } else {
        let mut rmod_ = crate::util::register_path_to_ty(base.register(), span);
        rmod_.path.segments.push(path_segment(base_ident.clone()));
        rmod_
    };
    Ok(path)
}

fn lookup_filter(
    evs: &[(EnumeratedValues, Option<EnumPath>)],
    usage: Usage,
) -> Option<&(EnumeratedValues, Option<EnumPath>)> {
    evs.iter()
        .find(|evsbase| evsbase.0.usage == Some(usage))
        .or_else(|| evs.first())
}
