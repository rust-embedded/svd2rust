use crate::svd::{
    self, Access, BitRange, DimElement, EnumeratedValue, EnumeratedValues, Field, MaybeArray,
    ModifiedWriteValues, ReadAction, Register, RegisterProperties, Usage, WriteConstraint,
    WriteConstraintRange,
};
use core::u64;
use log::warn;
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream};
use quote::quote;
use std::collections::HashSet;
use std::fmt::Write;
use std::{borrow::Cow, collections::BTreeMap};
use svd_parser::expand::{
    derive_enumerated_values, derive_field, BlockPath, EnumPath, FieldPath, Index, RegisterPath,
};

use crate::config::Config;
use crate::util::{
    self, ident, ident_to_path, path_segment, type_path, unsuffixed, DimSuffix, FullName, U32Ext,
};
use anyhow::{anyhow, Result};
use syn::punctuated::Punctuated;

fn regspec(name: &str, config: &Config, span: Span) -> Ident {
    ident(name, config, "register_spec", span)
}

fn field_accessor(name: &str, config: &Config, span: Span) -> Ident {
    const INTERNALS: [&str; 2] = ["bits", "set"];
    let sc = config
        .ident_formats
        .get("field_accessor")
        .unwrap()
        .sanitize(name);
    Ident::new(
        &(if INTERNALS.contains(&sc.as_ref()) {
            sc + "_"
        } else {
            sc
        }),
        span,
    )
}

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
                name = info
                    .fullname(config.ignore_groups)
                    .expand_dim(&index)
                    .into()
            }
        }
    }
    let span = Span::call_site();
    let reg_ty = ident(&name, config, "register", span);
    let doc_alias = (reg_ty.to_string().as_str() != name).then(|| quote!(#[doc(alias = #name)]));
    let mod_ty = ident(&name, config, "register_mod", span);
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
            util::block_path_to_ty(&dpath.block, config, span)
        };
        let dname = util::name_of(index.registers.get(dpath).unwrap(), config.ignore_groups);
        let mut mod_derived = derived.clone();
        derived
            .path
            .segments
            .push(path_segment(ident(&dname, config, "register", span)));
        mod_derived
            .path
            .segments
            .push(path_segment(ident(&dname, config, "register_mod", span)));

        Ok(quote! {
            pub use #derived as #reg_ty;
            pub use #mod_derived as #mod_ty;
        })
    } else {
        let regspec_ty = regspec(&name, config, span);
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

        let rpath = path.new_register(&register.name);
        let mut alias_doc = format!(
            "{name} ({accs}) register accessor: {description}{}{}",
            api_docs(
                access.can_read(),
                access.can_write(),
                register.properties.reset_value.is_some(),
                &mod_ty,
                false,
                &register,
                &rpath,
                config,
            )?,
            read_action_docs(access.can_read(), register.read_action),
        );
        alias_doc +=
            format!("\n\nFor information about available fields see [`mod@{mod_ty}`] module")
                .as_str();
        let mut out = TokenStream::new();
        out.extend(quote! {
            #[doc = #alias_doc]
            #doc_alias
            pub type #reg_ty = crate::Reg<#mod_ty::#regspec_ty>;
        });
        let mod_items = render_register_mod(register, access, &rpath, index, config)?;

        out.extend(quote! {
            #[doc = #description]
            pub mod #mod_ty {
                #mod_items
            }
        });

        Ok(out)
    }
}

fn read_action_docs(can_read: bool, read_action: Option<ReadAction>) -> String {
    let mut doc = String::new();
    if can_read {
        if let Some(action) = read_action {
            doc.push_str("\n\n<div class=\"warning\">");
            doc.push_str(match action {
                ReadAction::Clear => "The register is <b>cleared</b> (set to zero) following a read operation.",
                ReadAction::Set => "The register is <b>set</b> (set to ones) following a read operation.",
                ReadAction::Modify => "The register is <b>modified</b> in some way after a read operation.",
                ReadAction::ModifyExternal => "One or more dependent resources other than the current register are immediately affected by a read operation.",
            });
            doc.push_str("</div>");
        }
    }
    doc
}

fn api_docs(
    can_read: bool,
    can_write: bool,
    can_reset: bool,
    module: &Ident,
    inmodule: bool,
    register: &Register,
    rpath: &RegisterPath,
    config: &Config,
) -> Result<String, std::fmt::Error> {
    fn method(s: &str) -> String {
        format!("[`{s}`](crate::Reg::{s})")
    }

    let mut doc = String::from("\n\n");

    if can_read {
        write!(
            doc,
            "You can {} this register and get [`{module}::R`]{}. ",
            method("read"),
            if inmodule { "(R)" } else { "" },
        )?;
    }

    if can_write {
        let mut methods = Vec::new();
        if can_reset {
            methods.push("reset");
            methods.push("write");
        }
        methods.push("write_with_zero");
        write!(
            doc,
            "You can {} this register using [`{module}::W`]{}. ",
            methods
                .iter()
                .map(|m| method(m))
                .collect::<Vec<_>>()
                .join(", "),
            if inmodule { "(W)" } else { "" },
        )?;
    }

    if can_read && can_write {
        write!(doc, "You can also {} this register. ", method("modify"))?;
    }

    doc.push_str("See [API](https://docs.rs/svd2rust/#read--modify--write-api).");

    if let Some(url) = config.html_url.as_ref() {
        let first_idx = if let Register::Array(_, dim) = &register {
            dim.indexes().next()
        } else {
            None
        };
        let rname = if let Some(idx) = first_idx {
            let idx = format!("[{idx}]");
            rpath.name.replace("[%s]", &idx).replace("%s", &idx)
        } else {
            rpath.name.to_string()
        };
        // TODO: support html_urls for registers in cluster
        if rpath.block.path.is_empty() {
            doc.push_str(&format!(
                "\n\nSee register [structure]({url}#{}:{})",
                rpath.peripheral(),
                rname
            ));
        }
    }

    Ok(doc)
}

pub fn render_register_mod(
    register: &Register,
    access: Access,
    rpath: &RegisterPath,
    index: &Index,
    config: &Config,
) -> Result<TokenStream> {
    let properties = &register.properties;
    let name = util::name_of(register, config.ignore_groups);
    let rname = &register.name;
    let span = Span::call_site();
    let regspec_ty = regspec(&name, config, span);
    let mod_ty = ident(&name, config, "register_mod", span);
    let rsize = properties
        .size
        .ok_or_else(|| anyhow!("Register {rname} has no `size` field"))?;
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
            warn!("Missing description for register {rname}");
            Default::default()
        }))
        .as_ref(),
    );

    let mut mod_items = TokenStream::new();

    let can_read = access.can_read();
    let can_write = access.can_write();
    let can_reset = properties.reset_value.is_some();

    if can_read {
        let desc = format!("Register `{rname}` reader");
        mod_items.extend(quote! {
            #[doc = #desc]
            pub type R = crate::R<#regspec_ty>;
        });
    }

    if can_write {
        let desc = format!("Register `{rname}` writer");
        mod_items.extend(quote! {
            #[doc = #desc]
            pub type W = crate::W<#regspec_ty>;
        });
    }

    let mut r_impl_items = TokenStream::new();
    let mut r_debug_impl = TokenStream::new();
    let mut w_impl_items = TokenStream::new();
    let mut zero_to_modify_fields_bitmap = 0;
    let mut one_to_modify_fields_bitmap = 0;

    let debug_feature = config
        .impl_debug_feature
        .as_ref()
        .map(|feature| quote!(#[cfg(feature=#feature)]));

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
                &regspec_ty,
                register.modified_write_values,
                access,
                properties,
                &mut mod_items,
                rpath,
                index,
                config,
            )?;
        }
    } else if !access.can_read() || register.read_action.is_some() {
        r_debug_impl.extend(quote! {
            #debug_feature
            impl core::fmt::Debug for crate::generic::Reg<#regspec_ty> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "(not readable)")
                }
            }
        });
    } else {
        // no register fields are defined so implement Debug to get entire register value
        r_debug_impl.extend(quote! {
            #debug_feature
            impl core::fmt::Debug for R {
                fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                    write!(f, "{}", self.bits())
                }
            }
        });
    }

    if can_read && !r_impl_items.is_empty() {
        mod_items.extend(quote! { impl R { #r_impl_items }});
    }
    if !r_debug_impl.is_empty() {
        mod_items.extend(quote! { #r_debug_impl });
    }

    if can_write {
        mod_items.extend(quote! {
            impl W { #w_impl_items }
        });
    }

    let doc = format!(
        "{description}{}{}",
        api_docs(can_read, can_write, can_reset, &mod_ty, true, register, rpath, config)?,
        read_action_docs(access.can_read(), register.read_action),
    );

    mod_items.extend(quote! {
        #[doc = #doc]
        pub struct #regspec_ty;

        impl crate::RegisterSpec for #regspec_ty {
            type Ux = #rty;
        }
    });

    if can_read {
        let doc = format!("`read()` method returns [`{mod_ty}::R`](R) reader structure",);
        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Readable for #regspec_ty {}
        });
    }
    if can_write {
        // the writer can be safe if:
        // * there is a single field that covers the entire register
        // * that field can represent all values
        // * the write constraints of the register allow full range of values
        let safe_ty = if let Safety::Safe = Safety::get(
            register
                .fields
                .as_ref()
                .and_then(|fields| fields.first())
                .and_then(|field| field.write_constraint)
                .as_ref(),
            rsize,
        ) {
            Safety::Safe
        } else if let Safety::Safe = Safety::get(register.write_constraint.as_ref(), rsize) {
            Safety::Safe
        } else {
            Safety::Unsafe
        };
        let safe_ty = safe_ty.ident(rsize);

        let doc = format!("`write(|w| ..)` method takes [`{mod_ty}::W`](W) writer structure",);

        let zero_to_modify_fields_bitmap = util::hex(zero_to_modify_fields_bitmap);
        let one_to_modify_fields_bitmap = util::hex(one_to_modify_fields_bitmap);

        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Writable for #regspec_ty {
                type Safety = crate::#safe_ty;
                const ZERO_TO_MODIFY_FIELDS_BITMAP: #rty = #zero_to_modify_fields_bitmap;
                const ONE_TO_MODIFY_FIELDS_BITMAP: #rty = #one_to_modify_fields_bitmap;
            }
        });
    }
    if let Some(rv) = properties.reset_value.map(util::hex) {
        let doc = format!("`reset()` method sets {} to value {rv}", register.name);
        mod_items.extend(quote! {
            #[doc = #doc]
            impl crate::Resettable for #regspec_ty {
                const RESET_VALUE: #rty = #rv;
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
    let regspec_ty = regspec(&name, config, span);
    let mut r_debug_impl = TokenStream::new();
    let debug_feature = config
        .impl_debug_feature
        .as_ref()
        .map(|feature| quote!(#[cfg(feature=#feature)]));

    // implement Debug for register readable fields that have no read side effects
    if access.can_read() && register.read_action.is_none() {
        r_debug_impl.extend(quote! {
            #debug_feature
            impl core::fmt::Debug for R
        });
        let mut fmt_outer_impl = TokenStream::new();
        fmt_outer_impl.extend(quote! {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result
        });
        let mut fmt_inner_impl = TokenStream::new();
        fmt_inner_impl.extend(quote! {
                f.debug_struct(#name)
        });
        for &f in cur_fields.iter() {
            let field_access = match &f.access {
                Some(a) => a,
                None => access,
            };
            log::debug!("register={} field={}", name, f.name);
            if field_access.can_read() && f.read_action.is_none() {
                if let Field::Array(_, de) = &f {
                    for suffix in de.indexes() {
                        let f_name_n = field_accessor(&f.name.expand_dim(&suffix), config, span);
                        let f_name_n_s = format!("{f_name_n}");
                        fmt_inner_impl.extend(quote! {
                            .field(#f_name_n_s, &self.#f_name_n())
                        });
                    }
                } else {
                    let f_name = f.name.remove_dim();
                    let f_name = field_accessor(&f_name, config, span);
                    let f_name_s = format!("{f_name}");
                    fmt_inner_impl.extend(quote! {
                        .field(#f_name_s, &self.#f_name())
                    });
                }
            }
        }
        fmt_inner_impl.extend(quote! {
                    .finish()
        });
        let fmt_inner_group = Group::new(Delimiter::Brace, fmt_inner_impl);
        fmt_outer_impl.extend(quote! { #fmt_inner_group });
        let fmt_outer_group = Group::new(Delimiter::Brace, fmt_outer_impl);
        r_debug_impl.extend(quote! { #fmt_outer_group });
    } else if !access.can_read() || register.read_action.is_some() {
        r_debug_impl.extend(quote! {
            #debug_feature
            impl core::fmt::Debug for crate::generic::Reg<#regspec_ty> {
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

#[derive(Clone, Copy, Debug)]
pub enum EV<'a> {
    New(&'a EnumeratedValues),
    Derived(&'a EnumeratedValues, &'a EnumPath),
}

impl<'a> EV<'a> {
    fn values(&self) -> &EnumeratedValues {
        match self {
            Self::New(e) | Self::Derived(e, _) => e,
        }
    }
}

impl<'a> From<&'a (EnumeratedValues, Option<EnumPath>)> for EV<'a> {
    fn from(value: &'a (EnumeratedValues, Option<EnumPath>)) -> Self {
        match value.1.as_ref() {
            Some(base) => Self::Derived(&value.0, base),
            None => Self::New(&value.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RWEnum<'a> {
    ReadWriteCommon(EV<'a>),
    ReadWrite(ReadEnum<'a>, WriteEnum<'a>),
    Read(ReadEnum<'a>),
    Write(WriteEnum<'a>),
}

#[derive(Clone, Copy, Debug)]
pub enum ReadEnum<'a> {
    Enum(EV<'a>),
    Raw,
}

#[derive(Clone, Copy, Debug)]
pub enum WriteEnum<'a> {
    Enum(EV<'a>),
    Raw,
}

impl<'a> RWEnum<'a> {
    pub fn different_enums(&self) -> bool {
        matches!(self, Self::ReadWrite(ReadEnum::Enum(_), WriteEnum::Enum(_)))
    }
    pub fn read_write(&self) -> bool {
        matches!(self, Self::ReadWriteCommon(_) | Self::ReadWrite(_, _))
    }
    pub fn read_only(&self) -> bool {
        matches!(self, Self::Read(_))
    }
    pub fn can_read(&self) -> bool {
        self.read_write() || self.read_only()
    }
    pub fn write_only(&self) -> bool {
        matches!(self, Self::Write(_))
    }
    pub fn can_write(&self) -> bool {
        self.read_write() || self.write_only()
    }
    pub fn read_enum(&self) -> Option<EV<'a>> {
        match self {
            Self::ReadWriteCommon(e)
            | Self::ReadWrite(ReadEnum::Enum(e), _)
            | Self::Read(ReadEnum::Enum(e)) => Some(*e),
            _ => None,
        }
    }
    pub fn write_enum(&self) -> Option<EV<'a>> {
        match self {
            Self::ReadWriteCommon(e)
            | Self::ReadWrite(_, WriteEnum::Enum(e))
            | Self::Write(WriteEnum::Enum(e)) => Some(*e),
            _ => None,
        }
    }
    pub fn generate_write_enum(&self) -> bool {
        matches!(
            self,
            Self::ReadWrite(_, WriteEnum::Enum(_)) | Self::Write(WriteEnum::Enum(_))
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn fields(
    mut fields: Vec<&Field>,
    regspec_ty: &Ident,
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
    let mut read_enum_derives = HashSet::new();
    let mut write_enum_derives = HashSet::new();
    let mut reader_derives = HashSet::new();
    let mut writer_derives = HashSet::new();

    // TODO enumeratedValues
    let inline = quote! { #[inline(always)] };
    for &f in fields.iter() {
        let mut f = f.clone();
        let mut fdpath = None;
        if let Some(dpath) = f.derived_from.take() {
            fdpath = derive_field(&mut f, &dpath, rpath, index)?;
        }
        let fpath = rpath.new_field(&f.name);
        // TODO(AJM) - do we need to do anything with this range type?
        let BitRange { offset, width, .. } = f.bit_range;

        if f.is_single() && f.name.contains("%s") {
            return Err(anyhow!("incorrect field {}", f.name));
        }

        let name = f.name.remove_dim();
        let name_snake_case = field_accessor(
            if let Field::Array(
                _,
                DimElement {
                    dim_name: Some(dim_name),
                    ..
                },
            ) = &f
            {
                dim_name
            } else {
                &name
            },
            config,
            span,
        );
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

        let (use_cast, use_mask) = if let Some(size) = properties.size {
            let size = size.to_ty_width()?;
            (size != width.to_ty_width()?, size != width)
        } else {
            (true, true)
        };

        let mut lookup_results = Vec::new();
        for mut ev in f.enumerated_values.clone().into_iter() {
            let mut epath = None;
            let dpath = ev.derived_from.take();
            if let Some(dpath) = dpath {
                epath = Some(derive_enumerated_values(&mut ev, &dpath, &fpath, index)?);
                // TODO: remove this hack
                if let Some(epath) = epath.as_ref() {
                    ev = (*index.evs.get(epath).unwrap()).clone();
                }
            } else if let Some(path) = fdpath.as_ref() {
                epath = Some(
                    path.new_enum(
                        ev.name
                            .clone()
                            .unwrap_or_else(|| path.name.remove_dim().into()),
                    ),
                );
            }
            lookup_results.push((ev, epath));
        }

        let rwenum = match (
            can_read,
            lookup_filter(&lookup_results, Usage::Read),
            can_write,
            lookup_filter(&lookup_results, Usage::Write),
        ) {
            (true, Some(e1), true, Some(e2)) if e1.0 == e2.0 => RWEnum::ReadWriteCommon(e1.into()),
            (true, Some(e1), true, Some(e2)) => {
                RWEnum::ReadWrite(ReadEnum::Enum(e1.into()), WriteEnum::Enum(e2.into()))
            }
            (true, Some(e), true, None) => {
                RWEnum::ReadWrite(ReadEnum::Enum(e.into()), WriteEnum::Raw)
            }
            (true, None, true, Some(e)) => {
                RWEnum::ReadWrite(ReadEnum::Raw, WriteEnum::Enum(e.into()))
            }
            (true, Some(e), false, _) => RWEnum::Read(ReadEnum::Enum(e.into())),
            (true, None, false, _) => RWEnum::Read(ReadEnum::Raw),
            (false, _, true, Some(e)) => RWEnum::Write(WriteEnum::Enum(e.into())),
            (false, _, true, None) => RWEnum::Write(WriteEnum::Raw),
            (true, None, true, None) => RWEnum::ReadWrite(ReadEnum::Raw, WriteEnum::Raw),
            (false, _, false, _) => {
                return Err(anyhow!("Field {fpath} is not writtable or readable"))
            }
        };

        let brief_suffix = if let Field::Array(_, de) = &f {
            if let Some(range) = de.indexes_as_range() {
                let (start, end) = range.into_inner();
                format!("({start}-{end})")
            } else {
                let suffixes: Vec<_> = de.indexes().collect();
                format!("({})", suffixes.join(","))
            }
        } else {
            String::new()
        };

        // If this field can be read, generate read proxy structure and value structure.
        if can_read {
            // collect information on items in enumeration to generate it later.
            let mut enum_items = TokenStream::new();

            // if this is an enumeratedValues not derived from base, generate the enum structure
            // and implement functions for each value in enumeration.
            let value_read_ty = if let Some(ev) = rwenum.read_enum() {
                let derives;
                let fmt;
                if rwenum.different_enums() {
                    derives = &mut read_enum_derives;
                    fmt = "enum_read_name";
                } else {
                    derives = &mut enum_derives;
                    fmt = "enum_name";
                };
                // get the type of value structure. It can be generated from either name field
                // in enumeratedValues if it's an enumeration, or from field name directly if it's not.
                let value_read_ty = ident(
                    if config.field_names_for_enums {
                        &name
                    } else {
                        ev.values().name.as_deref().unwrap_or(&name)
                    },
                    config,
                    fmt,
                    span,
                );

                match ev {
                    EV::New(evs) => {
                        // parse enum variants from enumeratedValues svd record
                        let mut variants = Variant::from_enumerated_values(evs, config)?;

                        let map = enums_to_map(evs);
                        let mut def = evs
                            .default_value()
                            .and_then(|def| {
                                minimal_hole(&map, width)
                                    .map(|v| Variant::from_value(v, def, config))
                            })
                            .transpose()?;
                        if variants.len() == 1 << width {
                            def = None;
                        } else if variants.len() == (1 << width) - 1 {
                            if let Some(def) = def.take() {
                                variants.push(def);
                            }
                        }

                        // if there's no variant defined in enumeratedValues, generate enumeratedValues with new-type
                        // wrapper struct, and generate From conversation only.
                        // else, generate enumeratedValues into a Rust enum with functions for each variant.
                        if variants.is_empty() && def.is_none() {
                            // generate struct VALUE_READ_TY_A(fty) and From<fty> for VALUE_READ_TY_A.
                            add_with_no_variants(
                                mod_items,
                                &value_read_ty,
                                &fty,
                                &description,
                                rv,
                                config,
                            );
                        } else {
                            // do we have finite definition of this enumeration in svd? If not, the later code would
                            // return an Option when the value read from field does not match any defined values.
                            let has_reserved_variant;

                            // generate enum VALUE_READ_TY_A { ... each variants ... } and and From<fty> for VALUE_READ_TY_A.
                            if let Some(def) = def.as_ref() {
                                add_from_variants(
                                    mod_items,
                                    variants.iter().chain(std::iter::once(def)),
                                    &value_read_ty,
                                    &fty,
                                    &description,
                                    rv,
                                    config,
                                );
                                has_reserved_variant = false;
                            } else {
                                add_from_variants(
                                    mod_items,
                                    variants.iter(),
                                    &value_read_ty,
                                    &fty,
                                    &description,
                                    rv,
                                    config,
                                );
                                has_reserved_variant = evs.values.len() != (1 << width);
                            }

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
                            } else if let Some(v) = def.as_ref() {
                                let pc = &v.pc;
                                arms.extend(quote! {
                                    _ => #value_read_ty::#pc,
                                });
                            } else if 1 << width.to_ty_width()? != variants.len() {
                                arms.extend(quote! {
                                    _ => unreachable!(),
                                });
                            }

                            // prepare the `variant` function. This function would return field value in
                            // Rust structure; if we have reserved variant we return by Option.
                            let ret_ty = if has_reserved_variant {
                                quote!(Option<#value_read_ty>)
                            } else {
                                quote!(#value_read_ty)
                            };
                            enum_items.extend(quote! {
                                #[doc = "Get enumerated values variant"]
                                #inline
                                pub const fn variant(&self) -> #ret_ty {
                                    match self.bits {
                                        #arms
                                    }
                                }
                            });

                            // for each variant defined, we generate an `is_variant` function.
                            for v in &variants {
                                let pc = &v.pc;
                                let is_variant = &v.is_sc;

                                let doc = util::escape_special_chars(&util::respace(&v.doc));
                                enum_items.extend(quote! {
                                    #[doc = #doc]
                                    #inline
                                    pub fn #is_variant(&self) -> bool {
                                        *self == #value_read_ty::#pc
                                    }
                                });
                            }
                            if let Some(v) = def.as_ref() {
                                let pc = &v.pc;
                                let is_variant = &v.is_sc;

                                let doc = util::escape_special_chars(&util::respace(&v.doc));
                                enum_items.extend(quote! {
                                    #[doc = #doc]
                                    #inline
                                    pub fn #is_variant(&self) -> bool {
                                        matches!(self.variant(), #value_read_ty::#pc)
                                    }
                                });
                            }
                        }
                    }
                    EV::Derived(_, base) => {
                        let base_ident = if config.field_names_for_enums {
                            ident(&base.field().name.remove_dim(), config, fmt, span)
                        } else {
                            ident(&base.name, config, fmt, span)
                        };
                        if !derives.contains(&value_read_ty) {
                            let base_path = base_syn_path(base, &fpath, &base_ident, config)?;
                            mod_items.extend(quote! {
                                #[doc = #description]
                                pub use #base_path as #value_read_ty;
                            });
                        }
                    }
                }
                derives.insert(value_read_ty.clone());
                value_read_ty
            } else {
                // raw_field_value_read_ty
                fty.clone()
            };

            // get a brief description for this field
            // the suffix string from field name is removed in brief description.
            let field_reader_brief = format!("Field `{name}{brief_suffix}` reader - {description}");

            // name of read proxy type
            let reader_ty = ident(&name, config, "field_reader", span);

            match rwenum.read_enum() {
                Some(EV::New(_)) | None => {
                    // Generate the read proxy structure if necessary.

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
                        readerdoc.push_str("\n\n<div class=\"warning\">");
                        readerdoc.push_str(match action {
                            ReadAction::Clear => "The field is <b>cleared</b> (set to zero) following a read operation.",
                            ReadAction::Set => "The field is <b>set</b> (set to ones) following a read operation.",
                            ReadAction::Modify => "The field is <b>modified</b> in some way after a read operation.",
                            ReadAction::ModifyExternal => "One or more dependent resources other than the current field are immediately affected by a read operation.",
                        });
                        readerdoc.push_str("</div>");
                    }
                    mod_items.extend(quote! {
                        #[doc = #readerdoc]
                        pub type #reader_ty = #reader;
                    });
                }
                Some(EV::Derived(_, base)) => {
                    // if this value is derived from a base, generate `pub use` code for each read proxy
                    // and value if necessary.

                    // generate pub use field_1 reader as field_2 reader
                    let base_field = base.field.name.remove_dim();
                    let base_r = ident(&base_field, config, "field_reader", span);
                    if !reader_derives.contains(&reader_ty) {
                        let base_path = base_syn_path(base, &fpath, &base_r, config)?;
                        mod_items.extend(quote! {
                            #[doc = #field_reader_brief]
                            pub use #base_path as #reader_ty;
                        });
                        reader_derives.insert(reader_ty.clone());
                    }
                }
            }

            // Generate field reader accessors
            let cast = if width == 1 {
                quote! { != 0 }
            } else {
                quote! { as #fty }
            };

            if let Field::Array(f, de) = &f {
                let increment = de.dim_increment;
                let doc = description.expand_dim(&brief_suffix);
                let first_name = svd::array::names(f, de).next().unwrap();
                let note = format!("<div class=\"warning\">`n` is number of field in register. `n == 0` corresponds to `{first_name}` field.</div>");
                let offset_calc = calculate_offset(increment, offset, true);
                let value = quote! { ((self.bits >> #offset_calc) & #hexmask) #cast };
                let dim = unsuffixed(de.dim);
                let name_snake_case_iter = Ident::new(&format!("{name_snake_case}_iter"), span);
                r_impl_items.extend(quote! {
                    #[doc = #doc]
                    #[doc = ""]
                    #[doc = #note]
                    #inline
                    pub fn #name_snake_case(&self, n: u8) -> #reader_ty {
                        #[allow(clippy::no_effect)]
                        [(); #dim][n as usize];
                        #reader_ty::new ( #value )
                    }
                    #[doc = "Iterator for array of:"]
                    #[doc = #doc]
                    #inline
                    pub fn #name_snake_case_iter(&self) -> impl Iterator<Item = #reader_ty> + '_ {
                        (0..#dim).map(move |n| #reader_ty::new ( #value ))
                    }
                });

                for fi in svd::field::expand(f, de) {
                    let sub_offset = fi.bit_offset() as u64;
                    let value = if sub_offset != 0 {
                        let sub_offset = &unsuffixed(sub_offset);
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
                    let name_snake_case_n = field_accessor(&fi.name, config, span);
                    let doc = description_with_bits(
                        fi.description.as_deref().unwrap_or(&fi.name),
                        sub_offset,
                        width,
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
                let value = if offset != 0 {
                    let offset = &unsuffixed(offset);
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
            let mut proxy_items = TokenStream::new();
            let mut safety = Safety::get(f.write_constraint.as_ref(), width);

            // if we writes to enumeratedValues, generate its structure if it differs from read structure.
            let value_write_ty = if let Some(ev) = rwenum.write_enum() {
                let derives;
                let fmt;
                if rwenum.different_enums() {
                    derives = &mut write_enum_derives;
                    fmt = "enum_write_name";
                } else {
                    derives = &mut enum_derives;
                    fmt = "enum_name";
                };
                let value_write_ty = ident(
                    if config.field_names_for_enums {
                        &name
                    } else {
                        ev.values().name.as_deref().unwrap_or(&name)
                    },
                    config,
                    fmt,
                    span,
                );

                match ev {
                    EV::New(evs) => {
                        // parse variants from enumeratedValues svd record
                        let mut variants = Variant::from_enumerated_values(evs, config)?;
                        let map = enums_to_map(evs);
                        let mut def = evs
                            .default_value()
                            .and_then(|def| {
                                minimal_hole(&map, width)
                                    .map(|v| Variant::from_value(v, def, config))
                            })
                            .transpose()?;
                        // if the write structure is finite, it can be safely written.
                        if variants.len() == 1 << width {
                            safety = Safety::Safe;
                        } else if let Some(def) = def.take() {
                            variants.push(def);
                            safety = Safety::Safe;
                        }

                        // generate write value structure and From conversation if we can't reuse read value structure.
                        if rwenum.generate_write_enum() {
                            if variants.is_empty() {
                                add_with_no_variants(
                                    mod_items,
                                    &value_write_ty,
                                    &fty,
                                    &description,
                                    rv,
                                    config,
                                );
                            } else {
                                add_from_variants(
                                    mod_items,
                                    variants.iter(),
                                    &value_write_ty,
                                    &fty,
                                    &description,
                                    rv,
                                    config,
                                );
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
                                pub fn #sc(self) -> &'a mut crate::W<REG> {
                                    self.variant(#value_write_ty::#pc)
                                }
                            });
                        }
                    }
                    EV::Derived(_, base) => {
                        let base_ident = if config.field_names_for_enums {
                            ident(&base.field().name.remove_dim(), config, fmt, span)
                        } else {
                            ident(&base.name, config, fmt, span)
                        };
                        if rwenum.generate_write_enum() && !derives.contains(&value_write_ty) {
                            let base_path = base_syn_path(base, &fpath, &base_ident, config)?;
                            mod_items.extend(quote! {
                                #[doc = #description]
                                pub use #base_path as #value_write_ty;
                            });
                        }
                    }
                }
                derives.insert(value_write_ty.clone());
                value_write_ty
            } else {
                // raw_field_value_write_ty
                fty.clone()
            };

            let mwv = f.modified_write_values.or(rmwv).unwrap_or_default();

            // gets a brief of write proxy
            let field_writer_brief = format!("Field `{name}{brief_suffix}` writer - {description}");

            // name of write proxy type
            let writer_ty = ident(&name, config, "field_writer", span);

            // Generate writer structure by type alias to generic write proxy structure.
            match rwenum.write_enum() {
                Some(EV::New(_)) | None => {
                    let proxy = if width == 1 {
                        use ModifiedWriteValues::*;
                        let wproxy = Ident::new(
                            match mwv {
                                Modify | Set | Clear => "BitWriter",
                                OneToSet => "BitWriter1S",
                                ZeroToClear => "BitWriter0C",
                                OneToClear => "BitWriter1C",
                                ZeroToSet => "BitWriter0S",
                                OneToToggle => "BitWriter1T",
                                ZeroToToggle => "BitWriter0T",
                            },
                            span,
                        );
                        if value_write_ty == "bool" {
                            quote! { crate::#wproxy<'a, REG> }
                        } else {
                            quote! { crate::#wproxy<'a, REG, #value_write_ty> }
                        }
                    } else {
                        let wproxy = Ident::new("FieldWriter", span);
                        let uwidth = &unsuffixed(width);
                        if value_write_ty == "u8" && safety != Safety::Safe {
                            quote! { crate::#wproxy<'a, REG, #uwidth> }
                        } else if safety != Safety::Safe {
                            quote! { crate::#wproxy<'a, REG, #uwidth, #value_write_ty> }
                        } else {
                            let safe_ty = safety.ident(width);
                            quote! { crate::#wproxy<'a, REG, #uwidth, #value_write_ty, crate::#safe_ty> }
                        }
                    };
                    mod_items.extend(quote! {
                        #[doc = #field_writer_brief]
                        pub type #writer_ty<'a, REG> = #proxy;
                    });
                }
                Some(EV::Derived(_, base)) => {
                    // generate pub use field_1 writer as field_2 writer
                    let base_field = base.field.name.remove_dim();
                    let base_w = ident(&base_field, config, "field_writer", span);
                    if !writer_derives.contains(&writer_ty) {
                        let base_path = base_syn_path(base, &fpath, &base_w, config)?;
                        mod_items.extend(quote! {
                            #[doc = #field_writer_brief]
                            pub use #base_path as #writer_ty;
                        });
                        writer_derives.insert(writer_ty.clone());
                    }
                }
            }

            // generate proxy items from collected information
            if !proxy_items.is_empty() {
                mod_items.extend(if width == 1 {
                    quote! {
                        impl<'a, REG> #writer_ty<'a, REG>
                        where
                            REG: crate::Writable + crate::RegisterSpec,
                        {
                            #proxy_items
                        }
                    }
                } else {
                    quote! {
                        impl<'a, REG> #writer_ty<'a, REG>
                        where
                            REG: crate::Writable + crate::RegisterSpec,
                            REG::Ux: From<#fty>
                        {
                            #proxy_items
                        }
                    }
                });
            }

            // Generate field writer accessors
            if let Field::Array(f, de) = &f {
                let increment = de.dim_increment;
                let offset_calc = calculate_offset(increment, offset, false);
                let doc = &description.expand_dim(&brief_suffix);
                let first_name = svd::array::names(f, de).next().unwrap();
                let note = format!("<div class=\"warning\">`n` is number of field in register. `n == 0` corresponds to `{first_name}` field.</div>");
                let dim = unsuffixed(de.dim);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #[doc = ""]
                    #[doc = #note]
                    #inline
                    #[must_use]
                    pub fn #name_snake_case(&mut self, n: u8) -> #writer_ty<#regspec_ty> {
                        #[allow(clippy::no_effect)]
                        [(); #dim][n as usize];
                        #writer_ty::new(self, #offset_calc)
                    }
                });

                for fi in svd::field::expand(f, de) {
                    let sub_offset = fi.bit_offset() as u64;
                    let name_snake_case_n = field_accessor(&fi.name, config, span);
                    let doc = description_with_bits(
                        fi.description.as_deref().unwrap_or(&fi.name),
                        sub_offset,
                        width,
                    );
                    let sub_offset = unsuffixed(sub_offset);

                    w_impl_items.extend(quote! {
                        #[doc = #doc]
                        #inline
                        #[must_use]
                        pub fn #name_snake_case_n(&mut self) -> #writer_ty<#regspec_ty> {
                            #writer_ty::new(self, #sub_offset)
                        }
                    });
                }
            } else {
                let doc = description_with_bits(description_raw, offset, width);
                let offset = unsuffixed(offset);
                w_impl_items.extend(quote! {
                    #[doc = #doc]
                    #inline
                    #[must_use]
                    pub fn #name_snake_case(&mut self) -> #writer_ty<#regspec_ty> {
                        #writer_ty::new(self, #offset)
                    }
                });
            }

            // Update register modify bit masks
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Safety {
    Unsafe,
    Range(WriteConstraintRange),
    Safe,
}

impl Safety {
    fn get(write_constraint: Option<&WriteConstraint>, width: u32) -> Self {
        match &write_constraint {
            Some(&WriteConstraint::Range(range))
                if range.min == 0 && range.max == u64::MAX >> (64 - width) =>
            {
                // the SVD has acknowledged that it's safe to write
                // any value that can fit in the field
                Self::Safe
            }
            None if width == 1 => {
                // the field is one bit wide, so we assume it's legal to write
                // either value into it or it wouldn't exist; despite that
                // if a writeConstraint exists then respect it
                Self::Safe
            }
            Some(&WriteConstraint::Range(range)) => Self::Range(range),
            _ => Self::Unsafe,
        }
    }
    fn ident(&self, width: u32) -> TokenStream {
        match self {
            Self::Safe => quote!(Safe),
            Self::Unsafe => quote!(Unsafe),
            Self::Range(range) => {
                let min = unsuffixed(range.min);
                let max = unsuffixed(range.max);
                if range.min == 0 {
                    quote!(RangeTo<#max>)
                } else if range.max == u64::MAX >> (64 - width) {
                    quote!(RangeFrom<#min>)
                } else {
                    quote!(Range<#min, #max>)
                }
            }
        }
    }
}

struct Variant {
    doc: String,
    pc: Ident,
    is_sc: Ident,
    sc: Ident,
    value: u64,
}

impl Variant {
    fn from_enumerated_values(evs: &EnumeratedValues, config: &Config) -> Result<Vec<Self>> {
        evs.values
            .iter()
            // filter out all reserved variants, as we should not
            // generate code for them
            .filter(|ev| ev.name.to_lowercase() != "reserved" && !ev.is_default())
            .map(|ev| {
                let value = ev
                    .value
                    .ok_or_else(|| anyhow!("EnumeratedValue {} has no `<value>` entry", ev.name))?;
                Self::from_value(value, ev, config)
            })
            .collect()
    }
    fn from_value(value: u64, ev: &EnumeratedValue, config: &Config) -> Result<Self> {
        let span = Span::call_site();
        let case = config.ident_formats.get("enum_value_accessor").unwrap();
        let nksc = case.apply(&ev.name);
        let is_sc = Ident::new(
            &if nksc.to_string().starts_with('_') {
                format!("is{nksc}")
            } else {
                format!("is_{nksc}")
            },
            span,
        );
        let sc = case.sanitize(&ev.name);
        const INTERNALS: [&str; 6] = ["bit", "bits", "clear_bit", "set", "set_bit", "variant"];
        let sc = Ident::new(
            &(if INTERNALS.contains(&sc.as_ref()) {
                sc + "_"
            } else {
                sc
            }),
            span,
        );
        Ok(Variant {
            doc: ev
                .description
                .clone()
                .unwrap_or_else(|| format!("`{value:b}`")),
            pc: ident(&ev.name, config, "enum_value", span),
            is_sc,
            sc,
            value,
        })
    }
}

fn add_with_no_variants(
    mod_items: &mut TokenStream,
    pc: &Ident,
    fty: &Ident,
    desc: &str,
    reset_value: Option<u64>,
    config: &Config,
) {
    let defmt = config
        .impl_defmt
        .as_ref()
        .map(|feature| quote!(#[cfg_attr(feature = #feature, derive(defmt::Format))]));

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
        #defmt
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

fn add_from_variants<'a>(
    mod_items: &mut TokenStream,
    variants: impl Iterator<Item = &'a Variant>,
    pc: &Ident,
    fty: &Ident,
    desc: &str,
    reset_value: Option<u64>,
    config: &Config,
) {
    let defmt = config
        .impl_defmt
        .as_ref()
        .map(|feature| quote!(#[cfg_attr(feature = #feature, derive(defmt::Format))]));

    let (repr, cast) = if fty == "bool" {
        (quote! {}, quote! { variant as u8 != 0 })
    } else {
        (quote! { #[repr(#fty)] }, quote! { variant as _ })
    };

    let mut vars = TokenStream::new();
    for v in variants.map(|v| {
        let desc = util::escape_special_chars(&util::respace(&format!("{}: {}", v.value, v.doc)));
        let pcv = &v.pc;
        let pcval = &unsuffixed(v.value);
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
        #defmt
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
            impl crate::IsEnum for #pc {}
        });
    }
}

fn calculate_offset(increment: u32, offset: u64, with_parentheses: bool) -> TokenStream {
    let mut res = quote! { n };
    if increment != 1 {
        let increment = unsuffixed(increment);
        res = quote! { #res * #increment };
    }
    if offset != 0 {
        let offset = &unsuffixed(offset);
        res = quote! { #res + #offset };
    }
    let single_ident = (increment == 1) && (offset == 0);
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
    config: &Config,
) -> Result<syn::TypePath, syn::Error> {
    let span = Span::call_site();
    let path = if base.register() == fpath.register() {
        ident_to_path(base_ident.clone())
    } else if base.register().block == fpath.register().block {
        let mut segments = Punctuated::new();
        segments.push(path_segment(Ident::new("super", span)));
        segments.push(path_segment(ident(
            &base.register().name.remove_dim(),
            config,
            "register_mod",
            span,
        )));
        segments.push(path_segment(base_ident.clone()));
        type_path(segments)
    } else {
        let mut rmod_ = crate::util::register_path_to_ty(base.register(), config, span);
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

fn enums_to_map(evs: &EnumeratedValues) -> BTreeMap<u64, &EnumeratedValue> {
    let mut map = BTreeMap::new();
    for ev in &evs.values {
        if let Some(v) = ev.value {
            map.insert(v, ev);
        }
    }
    map
}

fn minimal_hole(map: &BTreeMap<u64, &EnumeratedValue>, width: u32) -> Option<u64> {
    (0..(1u64 << width)).find(|&v| !map.contains_key(&v))
}
