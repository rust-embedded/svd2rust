use regex::Regex;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use svd_parser::expand::{
    derive_cluster, derive_peripheral, derive_register, BlockPath, Index, RegisterPath,
};

use crate::svd::{
    array::names, Cluster, ClusterInfo, MaybeArray, Peripheral, Register, RegisterCluster,
    RegisterInfo,
};
use log::{debug, trace, warn};
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{punctuated::Punctuated, Token};

use crate::util::{
    self, array_proxy_type, name_to_ty, new_syn_u32, path_segment, type_path, unsuffixed, Config,
    FullName, ToSanitizedCase, BITS_PER_BYTE,
};
use anyhow::{anyhow, bail, Context, Result};

use crate::generate::register;

pub fn render(p_original: &Peripheral, index: &Index, config: &Config) -> Result<TokenStream> {
    let mut out = TokenStream::new();

    let mut p = p_original.clone();
    let mut path = None;
    let dpath = p.derived_from.take();
    if let Some(dpath) = dpath {
        path = derive_peripheral(&mut p, &dpath, index)?;
    }

    let name = util::name_of(&p, config.ignore_groups);
    let span = Span::call_site();
    let name_str = name.to_sanitized_constant_case();
    let name_constant_case = Ident::new(&name_str, span);
    let address = util::hex(p.base_address);
    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));

    let name_snake_case = name.to_snake_case_ident(span);
    let (derive_regs, base, path) = if let Some(path) = path {
        (true, path.peripheral.to_snake_case_ident(span), path)
    } else {
        (false, name_snake_case.clone(), BlockPath::new(&p.name))
    };

    let mut feature_attribute = TokenStream::new();
    if config.feature_group && p.group_name.is_some() {
        let feature_name = p.group_name.as_ref().unwrap().to_sanitized_snake_case();
        feature_attribute.extend(quote! { #[cfg(feature = #feature_name)] });
    };

    match &p {
        Peripheral::Array(p, dim) => {
            let names: Vec<Cow<str>> = names(p, dim).map(|n| n.into()).collect();
            let names_str = names.iter().map(|n| n.to_sanitized_constant_case());
            let names_constant_case = names_str.clone().map(|n| Ident::new(&n, span));
            let addresses =
                (0..=dim.dim).map(|i| util::hex(p.base_address + (i * dim.dim_increment) as u64));
            let snake_names = names
                .iter()
                .map(|p_name| p_name.to_sanitized_snake_case())
                .collect::<Vec<_>>();
            let feature_attribute_n = snake_names.iter().map(|p_snake| {
                let mut feature_attribute = feature_attribute.clone();
                if config.feature_peripheral {
                    feature_attribute.extend(quote! { #[cfg(feature = #p_snake)] })
                };
                feature_attribute
            });
            // Insert the peripherals structure
            out.extend(quote! {
                #(
                    #[doc = #description]
                    #feature_attribute_n
                    pub struct #names_constant_case { _marker: PhantomData<*const ()> }

                    #feature_attribute_n
                    unsafe impl Send for #names_constant_case {}

                    #feature_attribute_n
                    impl #names_constant_case {
                        ///Pointer to the register block
                        pub const PTR: *const #base::RegisterBlock = #addresses as *const _;

                        ///Return the pointer to the register block
                        #[inline(always)]
                        pub const fn ptr() -> *const #base::RegisterBlock {
                            Self::PTR
                        }
                    }

                    #feature_attribute_n
                    impl Deref for #names_constant_case {
                        type Target = #base::RegisterBlock;

                        #[inline(always)]
                        fn deref(&self) -> &Self::Target {
                            unsafe { &*Self::PTR }
                        }
                    }

                    #feature_attribute_n
                    impl core::fmt::Debug for #names_constant_case {
                        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                            f.debug_struct(#names_str).finish()
                        }
                    }
                )*
            });

            let feature_any_attribute = quote! {#[cfg(any(#(feature = #snake_names),*))]};

            // Derived peripherals may not require re-implementation, and will instead
            // use a single definition of the non-derived version.
            if derive_regs {
                // re-export the base module to allow deriveFrom this one
                out.extend(quote! {
                    #[doc = #description]
                    #feature_any_attribute
                    pub use self::#base as #name_snake_case;
                });
                return Ok(out);
            }
        }
        Peripheral::Single(_) => {
            let p_snake = name.to_sanitized_snake_case();
            if config.feature_peripheral {
                feature_attribute.extend(quote! { #[cfg(feature = #p_snake)] })
            };
            // Insert the peripheral structure
            out.extend(quote! {
                #[doc = #description]
                #feature_attribute
                pub struct #name_constant_case { _marker: PhantomData<*const ()> }

                #feature_attribute
                unsafe impl Send for #name_constant_case {}

                #feature_attribute
                impl #name_constant_case {
                    ///Pointer to the register block
                    pub const PTR: *const #base::RegisterBlock = #address as *const _;

                    ///Return the pointer to the register block
                    #[inline(always)]
                    pub const fn ptr() -> *const #base::RegisterBlock {
                        Self::PTR
                    }
                }

                #feature_attribute
                impl Deref for #name_constant_case {
                    type Target = #base::RegisterBlock;

                    #[inline(always)]
                    fn deref(&self) -> &Self::Target {
                        unsafe { &*Self::PTR }
                    }
                }

                #feature_attribute
                impl core::fmt::Debug for #name_constant_case {
                    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                        f.debug_struct(#name_str).finish()
                    }
                }
            });

            // Derived peripherals may not require re-implementation, and will instead
            // use a single definition of the non-derived version.
            if derive_regs {
                // re-export the base module to allow deriveFrom this one
                out.extend(quote! {
                    #[doc = #description]
                    #feature_attribute
                    pub use self::#base as #name_snake_case;
                });
                return Ok(out);
            }
        }
    }

    let description = util::escape_brackets(
        util::respace(p.description.as_ref().unwrap_or(&name.as_ref().to_owned())).as_ref(),
    );

    // Build up an alternate erc list by expanding any derived registers/clusters
    // erc: *E*ither *R*egister or *C*luster
    let mut ercs = p.registers.take().unwrap_or_default();

    // No `struct RegisterBlock` can be generated
    if ercs.is_empty() {
        // Drop the definition of the peripheral
        return Ok(TokenStream::new());
    }

    debug!("Checking derivation information");
    let derive_infos = check_erc_derive_infos(&mut ercs, &path, index, config)?;
    let zipped = ercs.iter_mut().zip(derive_infos.iter());
    for (mut erc, derive_info) in zipped {
        match &mut erc {
            &mut RegisterCluster::Register(register) => match derive_info {
                DeriveInfo::Implicit(rpath) => {
                    debug!(
                        "register {} implicitly derives from {}",
                        register.name, rpath.name
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }

    debug!("Pushing cluster & register information into output");
    // Push all cluster & register related information into the peripheral module

    let mod_items = render_ercs(&mut ercs, &derive_infos, &path, index, config)?;

    // Push any register or cluster blocks into the output
    debug!(
        "Pushing {} register or cluster blocks into output",
        ercs.len()
    );
    let reg_block = register_or_cluster_block(&ercs, &derive_infos, None, None, config)?;

    let open = Punct::new('{', Spacing::Alone);
    let close = Punct::new('}', Spacing::Alone);

    out.extend(quote! {
        #[doc = #description]
        #feature_attribute
        pub mod #name_snake_case #open
    });

    out.extend(reg_block);
    out.extend(mod_items);

    close.to_tokens(&mut out);

    p.registers = Some(ercs);

    Ok(out)
}

/// An enum describing the derivation status of an erc, which allows for disjoint arrays to be
/// implicitly derived from a common type.
#[derive(Debug, PartialEq)]
enum DeriveInfo {
    Root,
    Explicit(RegisterPath),
    Implicit(RegisterPath),
    Cluster, // don't do anything different for clusters
}

impl Default for DeriveInfo {
    fn default() -> Self {
        DeriveInfo::Root
    }
}

impl fmt::Display for DeriveInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Debug)]
pub struct ArrayAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub basename: Ident,
    pub i: syn::LitInt,
}

impl ArrayAccessor {
    pub fn to_tokens(&self, method: bool) -> TokenStream {
        let parens = method.then(|| quote! {()});
        let doc = &self.doc;
        let name = &self.name;
        let ty = &self.ty;
        let basename = &self.basename;
        let i = &self.i;
        quote! {
            #[doc = #doc]
            #[inline(always)]
            pub fn #name(&self) -> &#ty {
                &self.#basename #parens[#i]
            }
        }
    }
}

#[derive(Clone, Debug)]
struct RegisterBlockField {
    syn_field: syn::Field,
    description: String,
    offset: u32,
    size: u32,
    accessors: Vec<ArrayAccessor>,
}

#[derive(Clone, Debug)]
struct Region {
    rbfs: Vec<RegisterBlockField>,
    offset: u32,
    end: u32,
    /// This is only used for regions with `rbfs.len() > 1`
    pub ident: Option<String>,
}

impl Region {
    fn shortest_ident(&self) -> Option<String> {
        let mut idents: Vec<_> = self
            .rbfs
            .iter()
            .filter_map(|f| f.syn_field.ident.as_ref().map(|ident| ident.to_string()))
            .collect();
        if idents.is_empty() {
            return None;
        }
        idents.sort_by(|a, b| {
            // Sort by length and then content
            match a.len().cmp(&b.len()) {
                Ordering::Equal => a.cmp(b),
                cmp => cmp,
            }
        });
        Some(idents[0].to_owned())
    }

    fn common_ident(&self) -> Option<String> {
        // https://stackoverflow.com/a/40296745/4284367
        fn split_keep(text: &str) -> Vec<&str> {
            let mut result = Vec::new();
            let mut last = 0;
            for (index, matched) in
                text.match_indices(|c: char| c.is_numeric() || !c.is_alphabetic())
            {
                if last != index {
                    result.push(&text[last..index]);
                }
                result.push(matched);
                last = index + matched.len();
            }
            if last < text.len() {
                result.push(&text[last..]);
            }
            result
        }

        let idents: Vec<_> = self
            .rbfs
            .iter()
            .filter_map(|f| f.syn_field.ident.as_ref().map(|ident| ident.to_string()))
            .collect();

        if idents.is_empty() {
            return None;
        }

        let x: Vec<_> = idents.iter().map(|i| split_keep(i)).collect();
        let mut index = 0;
        let first = &x[0];
        // Get first elem, check against all other, break on mismatch
        'outer: while index < first.len() {
            for ident_match in x.iter().skip(1) {
                if let Some(match_) = ident_match.get(index) {
                    if match_ != &first[index] {
                        break 'outer;
                    }
                } else {
                    break 'outer;
                }
            }
            index += 1;
        }
        if index <= 1 {
            None
        } else {
            Some(match first.get(index) {
                Some(elem) if elem.chars().all(|c| c.is_numeric()) => {
                    first.iter().take(index).cloned().collect()
                }
                _ => first.iter().take(index - 1).cloned().collect(),
            })
        }
    }

    fn compute_ident(&self) -> Option<String> {
        if let Some(ident) = self.common_ident() {
            Some(ident)
        } else {
            self.shortest_ident()
        }
    }

    fn is_union(&self) -> bool {
        self.rbfs.len() > 1
    }
}

/// FieldRegions keeps track of overlapping field regions,
/// merging rbfs into appropriate regions as we process them.
/// This allows us to reason about when to create a union
/// rather than a struct.
#[derive(Default, Debug)]
struct FieldRegions {
    /// The set of regions we know about.  This is maintained
    /// in sorted order, keyed by Region::offset.
    regions: Vec<Region>,
}

impl FieldRegions {
    /// Track a field.  If the field overlaps with 1 or more existing
    /// entries, they will be merged together.
    fn add(&mut self, rbf: &RegisterBlockField) -> Result<()> {
        // When merging, this holds the indices in self.regions
        // that the input `rbf` will be merging with.
        let mut indices = Vec::new();

        let rbf_start = rbf.offset;
        let rbf_end = rbf_start + (rbf.size + BITS_PER_BYTE - 1) / BITS_PER_BYTE;

        // The region that we're going to insert
        let mut new_region = Region {
            rbfs: vec![rbf.clone()],
            offset: rbf.offset,
            end: rbf_end,
            ident: None,
        };

        // Locate existing region(s) that we intersect with and
        // fold them into the new region we're creating.  There
        // may be multiple regions that we intersect with, so
        // we keep looping to find them all.
        for (idx, f) in self.regions.iter_mut().enumerate() {
            let f_start = f.offset;
            let f_end = f.end;

            // Compute intersection range
            let begin = f_start.max(rbf_start);
            let end = f_end.min(rbf_end);

            if end > begin {
                // We're going to remove this element and fold it
                // into our new region
                indices.push(idx);

                // Expand the existing entry
                new_region.offset = new_region.offset.min(f_start);
                new_region.end = new_region.end.max(f_end);

                // And merge in the rbfs
                new_region.rbfs.append(&mut f.rbfs);
            }
        }

        // Now remove the entries that we collapsed together.
        // We do this in reverse order to ensure that the indices
        // are stable in the face of removal.
        for idx in indices.iter().rev() {
            self.regions.remove(*idx);
        }

        new_region.rbfs.sort_by_key(|f| f.offset);

        // maintain the regions ordered by starting offset
        let idx = self
            .regions
            .binary_search_by_key(&new_region.offset, |r| r.offset);
        match idx {
            Ok(idx) => {
                bail!(
                    "we shouldn't exist in the vec, but are at idx {} {:#?}\n{:#?}",
                    idx,
                    new_region,
                    self.regions
                );
            }
            Err(idx) => self.regions.insert(idx, new_region),
        };

        Ok(())
    }

    /// Resolves type name conflicts
    pub fn resolve_idents(&mut self) -> Result<()> {
        let idents: Vec<_> = {
            self.regions
                .iter_mut()
                .filter(|r| r.rbfs.len() > 1)
                .map(|r| {
                    r.ident = r.compute_ident();
                    r.ident.clone()
                })
                .collect()
        };
        self.regions
            .iter_mut()
            .filter(|r| r.ident.is_some())
            .filter(|r| {
                r.rbfs.len() > 1 && (idents.iter().filter(|&ident| ident == &r.ident).count() > 1)
            })
            .for_each(|r| {
                let new_ident = r.shortest_ident();
                warn!(
                    "Found type name conflict with region {:?}, renamed to {new_ident:?}",
                    r.ident
                );
                r.ident = new_ident;
            });
        Ok(())
    }
}

fn make_comment(size: u32, offset: u32, description: &str) -> String {
    let desc = util::escape_brackets(&util::respace(description));
    if size > 32 {
        let end = offset + size / 8;
        format!("0x{offset:02x}..0x{end:02x} - {desc}")
    } else {
        format!("0x{offset:02x} - {desc}")
    }
}

fn register_or_cluster_block(
    ercs: &[RegisterCluster],
    derive_infos: &[DeriveInfo],
    name: Option<&str>,
    size: Option<u32>,
    config: &Config,
) -> Result<TokenStream> {
    let mut rbfs = TokenStream::new();
    let mut accessors = TokenStream::new();

    let ercs_expanded = expand(ercs, derive_infos, config)
        .with_context(|| "Could not expand register or cluster block")?;

    // Locate conflicting regions; we'll need to use unions to represent them.
    let mut regions = FieldRegions::default();

    for reg_block_field in &ercs_expanded {
        regions.add(reg_block_field)?;
    }

    // We need to compute the idents of each register/union block first to make sure no conflicts exists.
    regions.resolve_idents()?;
    // The end of the region for which we previously emitted a rbf into `rbfs`
    let mut last_end = 0;

    let span = Span::call_site();
    for (i, region) in regions.regions.iter().enumerate() {
        // Check if we need padding
        let pad = region.offset - last_end;
        if pad != 0 {
            let name = Ident::new(&format!("_reserved{i}"), span);
            let pad = util::hex(pad as u64);
            rbfs.extend(quote! {
                #name : [u8; #pad],
            });
        }

        let mut region_rbfs = TokenStream::new();
        let is_region_a_union = region.is_union();

        for reg_block_field in &region.rbfs {
            let comment = make_comment(
                reg_block_field.size,
                reg_block_field.offset,
                &reg_block_field.description,
            );

            if is_region_a_union {
                let name = &reg_block_field.syn_field.ident;
                let ty = &reg_block_field.syn_field.ty;
                let offset = reg_block_field.offset as usize;
                accessors.extend(quote! {
                    #[doc = #comment]
                    #[inline(always)]
                    pub const fn #name(&self) -> &#ty {
                        unsafe { &*(self as *const Self).cast::<u8>().add(#offset).cast() }
                    }
                });
            } else {
                region_rbfs.extend(quote! {
                    #[doc = #comment]
                });

                reg_block_field.syn_field.to_tokens(&mut region_rbfs);
                Punct::new(',', Spacing::Alone).to_tokens(&mut region_rbfs);
            }
            accessors.extend(
                reg_block_field
                    .accessors
                    .iter()
                    .map(|a| a.to_tokens(is_region_a_union)),
            );
        }

        if !is_region_a_union {
            rbfs.extend(region_rbfs);
        } else {
            // Emit padding for the items that we're not emitting
            // as rbfs so that subsequent rbfs have the correct
            // alignment in the struct.  We could omit this and just
            // not updated `last_end`, so that the padding check in
            // the outer loop kicks in, but it is nice to be able to
            // see that the padding is attributed to a union when
            // visually inspecting the alignment in the struct.
            //
            // Include the computed ident for the union in the padding
            // name, along with the region number, falling back to
            // the offset and end in case we couldn't figure out a
            // nice identifier.
            let name = Ident::new(
                &format!(
                    "_reserved_{i}_{}",
                    region
                        .compute_ident()
                        .unwrap_or_else(|| format!("{}_{}", region.offset, region.end))
                ),
                span,
            );
            let pad = util::hex((region.end - region.offset) as u64);
            rbfs.extend(quote! {
                #name: [u8; #pad],
            })
        }
        last_end = region.end;
    }

    if let Some(size) = size {
        let pad = size
            .checked_sub(last_end)
            .ok_or_else(|| anyhow!("Incorrect block size"))?;
        if pad > 0 {
            let name = Ident::new("_reserved_end", span);
            let pad = util::hex(pad as u64);
            rbfs.extend(quote! {
                #name : [u8; #pad],
            });
        }
    }

    let name = if let Some(name) = name {
        name.to_constant_case_ident(span)
    } else {
        Ident::new("RegisterBlock", span)
    };

    let accessors = if !accessors.is_empty() {
        quote! {
            impl #name {
                #accessors
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        ///Register block
        #[repr(C)]
        pub struct #name {
            #rbfs
        }

        #accessors
    })
}

/// Expand a list of parsed `Register`s or `Cluster`s, and render them to
/// `RegisterBlockField`s containing `Field`s.
fn expand(
    ercs: &[RegisterCluster],
    derive_infos: &[DeriveInfo],
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut ercs_expanded = vec![];
    debug!("Expanding registers or clusters into Register Block Fields");
    let zipped = ercs.iter().zip(derive_infos.iter());

    for (erc, derive_info) in zipped {
        match &erc {
            RegisterCluster::Register(register) => {
                let reg_name = &register.name;
                let expanded_reg =
                    expand_register(register, derive_info, config).with_context(|| {
                        let descrip = register.description.as_deref().unwrap_or("No description");
                        format!(
                            "Error expanding register\nName: {reg_name}\nDescription: {descrip}"
                        )
                    })?;
                trace!("Register: {reg_name}");
                ercs_expanded.extend(expanded_reg);
            }
            RegisterCluster::Cluster(cluster) => {
                let cluster_name = &cluster.name;
                let expanded_cluster = expand_cluster(cluster, config).with_context(|| {
                    let descrip = cluster.description.as_deref().unwrap_or("No description");
                    format!("Error expanding cluster\nName: {cluster_name}\nDescription: {descrip}")
                })?;
                trace!("Cluster: {cluster_name}");
                ercs_expanded.extend(expanded_cluster);
            }
        };
    }

    ercs_expanded.sort_by_key(|x| x.offset);

    Ok(ercs_expanded)
}

/// Searches types using regex to find disjoint arrays which should implicitly derive from
/// another register. Returns a vector of `DeriveInfo` which should be `zip`ped with ercs when rendering.
fn check_erc_derive_infos(
    ercs: &mut [RegisterCluster],
    path: &BlockPath,
    index: &Index,
    config: &Config,
) -> Result<Vec<DeriveInfo>> {
    let mut ercs_type_info: Vec<(String, Option<Regex>, &RegisterCluster, &mut DeriveInfo)> =
        vec![];
    let mut derive_infos: Vec<DeriveInfo> = vec![];
    // fill the array so we can slice it (without implementing clone)
    for _i in 0..ercs.len() {
        derive_infos.push(DeriveInfo::default());
    }
    let derive_infos_slice = &mut derive_infos[0..ercs.len()];
    let zipped = ercs.iter_mut().zip(derive_infos_slice.iter_mut());
    for (mut erc, derive_info) in zipped {
        match &mut erc {
            &mut RegisterCluster::Register(register) => {
                let info_name = register.fullname(config.ignore_groups).to_string();
                let explicit_rpath = match &mut register.derived_from.clone() {
                    Some(dpath) => {
                        let (_, root) = find_root(&dpath, path, index)?;
                        Some(root)
                    }
                    None => None,
                };
                match register {
                    Register::Single(_) => {
                        let ty_name = info_name.to_string();
                        *derive_info = match explicit_rpath {
                            None => {
                                match compare_this_against_prev(
                                    &register,
                                    &ty_name,
                                    path,
                                    index,
                                    &ercs_type_info,
                                )? {
                                    Some(root) => {
                                        // make sure the matched register isn't already deriving from this register
                                        if ty_name == root.name {
                                            DeriveInfo::Root
                                        } else {
                                            DeriveInfo::Implicit(root)
                                        }
                                    }
                                    None => DeriveInfo::Root,
                                }
                            }
                            Some(rpath) => DeriveInfo::Explicit(rpath),
                        };
                        ercs_type_info.push((ty_name, None, erc, derive_info));
                    }

                    Register::Array(..) => {
                        // Only match integer indeces when searching for disjoint arrays
                        let re_string = util::replace_suffix(&info_name, "([0-9]+|%s)");
                        let re = Regex::new(format!("^{re_string}$").as_str()).or_else(|_| {
                            Err(anyhow!(
                                "Error creating regex for register {}",
                                register.name
                            ))
                        })?;
                        let ty_name = info_name.to_string(); // keep suffix for regex matching
                        *derive_info = match explicit_rpath {
                            None => {
                                match compare_this_against_prev(
                                    &register,
                                    &ty_name,
                                    path,
                                    index,
                                    &ercs_type_info,
                                )? {
                                    Some(root) => DeriveInfo::Implicit(root),
                                    None => compare_prev_against_this(
                                        &register,
                                        &ty_name,
                                        &re,
                                        path,
                                        index,
                                        &mut ercs_type_info,
                                    )?,
                                }
                            }
                            Some(rpath) => DeriveInfo::Explicit(rpath),
                        };
                        ercs_type_info.push((ty_name, Some(re), erc, derive_info));
                    }
                };
            }
            &mut RegisterCluster::Cluster(cluster) => {
                *derive_info = DeriveInfo::Cluster;
                ercs_type_info.push((cluster.name.to_string(), None, erc, derive_info));
            }
        };
    }
    Ok(derive_infos)
}

fn find_root(
    dpath: &str,
    path: &BlockPath,
    index: &Index,
) -> Result<(MaybeArray<RegisterInfo>, RegisterPath)> {
    let (dblock, dname) = RegisterPath::parse_str(dpath);
    let rdpath;
    let reg_path;
    let d = (if let Some(dblock) = dblock {
        reg_path = dblock.new_register(dname);
        rdpath = dblock;
        index.registers.get(&reg_path)
    } else {
        reg_path = path.new_register(dname);
        rdpath = path.clone();
        index.registers.get(&reg_path)
    })
    .ok_or_else(|| anyhow!("register {} not found", dpath))?;
    match d.derived_from.as_ref() {
        Some(dp) => find_root(dp, &rdpath, index),
        None => Ok(((*d).clone(), reg_path)),
    }
}

/// Compare the given type name against previous regexs, then inspect fields
fn compare_this_against_prev(
    reg: &MaybeArray<RegisterInfo>,
    ty_name: &str,
    path: &BlockPath,
    index: &Index,
    ercs_type_info: &Vec<(String, Option<Regex>, &RegisterCluster, &mut DeriveInfo)>,
) -> Result<Option<RegisterPath>> {
    for prev in ercs_type_info {
        let (prev_name, prev_regex, prev_erc, _prev_derive_info) = prev;
        if let RegisterCluster::Register(_) = prev_erc {
            if let Some(prev_re) = prev_regex {
                if prev_re.is_match(&ty_name) {
                    let (source_reg, rpath) = find_root(&prev_name, path, index)?;
                    if is_derivable(&source_reg, &reg) {
                        return Ok(Some(rpath));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// Compare the given type name against previous regexs, then inspect fields
fn compare_prev_against_this(
    reg: &MaybeArray<RegisterInfo>,
    ty_name: &String,
    re: &regex::Regex,
    path: &BlockPath,
    index: &Index,
    ercs_type_info: &mut Vec<(String, Option<Regex>, &RegisterCluster, &mut DeriveInfo)>,
) -> Result<DeriveInfo> {
    let mut my_derive_info = DeriveInfo::Root;
    // Check this type regex against previous names
    for prev in ercs_type_info {
        let (prev_name, _prev_regex, prev_erc, prev_derive_info) = prev;
        if let RegisterCluster::Register(prev_reg) = prev_erc {
            if let Register::Array(..) = prev_reg {
                // Arrays are covered with compare_this_against_prev
                continue;
            }
            if re.is_match(&prev_name) {
                let loop_derive_info = match prev_derive_info {
                    DeriveInfo::Root => {
                        // Get the RegisterPath for reg
                        let (_, implicit_rpath) = find_root(&ty_name, path, index)?;
                        if is_derivable(&prev_reg, &reg) {
                            **prev_derive_info = DeriveInfo::Implicit(implicit_rpath);
                        }
                        DeriveInfo::Root
                    }
                    DeriveInfo::Explicit(rpath) => {
                        let (source_reg, implicit_rpath) = find_root(&rpath.name, path, index)?;
                        if is_derivable(&source_reg, reg) {
                            DeriveInfo::Implicit(implicit_rpath)
                        } else {
                            DeriveInfo::Root
                        }
                    }
                    DeriveInfo::Implicit(rpath) => {
                        let (source_reg, _) = find_root(&rpath.name, path, index)?;
                        if is_derivable(&source_reg, reg) {
                            DeriveInfo::Implicit(rpath.clone())
                        } else {
                            DeriveInfo::Root
                        }
                    }
                    DeriveInfo::Cluster => {
                        return Err(anyhow!("register {} represented as cluster", prev_reg.name))
                    }
                };
                if let DeriveInfo::Root = my_derive_info {
                    if my_derive_info != loop_derive_info {
                        my_derive_info = loop_derive_info;
                    }
                }
            }
        }
    }
    Ok(my_derive_info)
}

fn is_derivable(
    source_reg: &MaybeArray<RegisterInfo>,
    target_reg: &MaybeArray<RegisterInfo>,
) -> bool {
    (source_reg.properties == target_reg.properties) && (source_reg.fields == target_reg.fields)
}

/// Calculate the size of a Cluster.  If it is an array, then the dimensions
/// tell us the size of the array.  Otherwise, inspect the contents using
/// [cluster_info_size_in_bits].
fn cluster_size_in_bits(cluster: &Cluster, config: &Config) -> Result<u32> {
    match cluster {
        Cluster::Single(info) => cluster_info_size_in_bits(info, config),
        // If the contained array cluster has a mismatch between the
        // dimIncrement and the size of the array items, then the array
        // will get expanded in expand_cluster below.  The overall size
        // then ends at the last array entry.
        Cluster::Array(info, dim) => {
            if dim.dim == 0 {
                return Ok(0); // Special case!
            }
            let last_offset = (dim.dim - 1) * dim.dim_increment * BITS_PER_BYTE;
            let last_size = cluster_info_size_in_bits(info, config);
            Ok(last_offset + last_size?)
        }
    }
}

/// Recursively calculate the size of a ClusterInfo. A cluster's size is the
/// maximum end position of its recursive children.
fn cluster_info_size_in_bits(info: &ClusterInfo, config: &Config) -> Result<u32> {
    let mut size = 0;

    for c in &info.children {
        let end = match c {
            RegisterCluster::Register(reg) => {
                let reg_size: u32 = expand_register(reg, &DeriveInfo::Root, config)?
                    .iter()
                    .map(|rbf| rbf.size)
                    .sum();

                (reg.address_offset * BITS_PER_BYTE) + reg_size
            }
            RegisterCluster::Cluster(clust) => {
                (clust.address_offset * BITS_PER_BYTE) + cluster_size_in_bits(clust, config)?
            }
        };

        size = size.max(end);
    }
    Ok(size)
}

/// Render a given cluster (and any children) into `RegisterBlockField`s
fn expand_cluster(cluster: &Cluster, config: &Config) -> Result<Vec<RegisterBlockField>> {
    let mut cluster_expanded = vec![];

    let cluster_size = cluster_info_size_in_bits(cluster, config)
        .with_context(|| format!("Can't calculate cluster {} size", cluster.name))?;
    let description = cluster
        .description
        .as_ref()
        .unwrap_or(&cluster.name)
        .to_string();

    let ty_name = if cluster.is_single() {
        cluster.name.to_string()
    } else {
        util::replace_suffix(&cluster.name, "")
    };
    let ty = name_to_ty(&ty_name);

    match cluster {
        Cluster::Single(info) => {
            let syn_field = new_syn_field(info.name.to_snake_case_ident(Span::call_site()), ty);
            cluster_expanded.push(RegisterBlockField {
                syn_field,
                description,
                offset: info.address_offset,
                size: cluster_size,
                accessors: Vec::new(),
            })
        }
        Cluster::Array(info, array_info) => {
            let increment_bits = array_info.dim_increment * BITS_PER_BYTE;
            if cluster_size > increment_bits {
                let cname = &cluster.name;
                return Err(anyhow!("Cluster {cname} has size {cluster_size} bits that is more then array increment {increment_bits} bits"));
            }
            let cluster_size = if config.max_cluster_size {
                increment_bits
            } else {
                cluster_size
            };
            let sequential_addresses = (array_info.dim == 1) || (cluster_size == increment_bits);

            // if dimIndex exists, test if it is a sequence of numbers from 0 to dim
            let sequential_indexes_from0 = array_info
                .indexes_as_range()
                .filter(|r| *r.start() == 0)
                .is_some();

            let convert_list = match config.keep_list {
                true => match &array_info.dim_name {
                    Some(dim_name) => dim_name.contains("[%s]"),
                    None => info.name.contains("[%s]"),
                },
                false => true,
            };

            let array_convertible = sequential_addresses && convert_list;

            if array_convertible {
                let accessors = if sequential_indexes_from0 {
                    Vec::new()
                } else {
                    let span = Span::call_site();
                    let mut accessors = Vec::new();
                    let nb_name_cs = ty_name.to_snake_case_ident(span);
                    for (i, idx) in array_info.indexes().enumerate() {
                        let idx_name =
                            util::replace_suffix(&info.name, &idx).to_snake_case_ident(span);
                        let comment = make_comment(
                            cluster_size,
                            info.address_offset + (i as u32) * cluster_size / 8,
                            &description,
                        );
                        let i = unsuffixed(i as _);
                        accessors.push(ArrayAccessor {
                            doc: comment,
                            name: idx_name,
                            ty: ty.clone(),
                            basename: nb_name_cs.clone(),
                            i,
                        });
                    }
                    accessors
                };
                let array_ty = new_syn_array(ty, array_info.dim);
                cluster_expanded.push(RegisterBlockField {
                    syn_field: new_syn_field(
                        ty_name.to_snake_case_ident(Span::call_site()),
                        array_ty,
                    ),
                    description,
                    offset: info.address_offset,
                    size: cluster_size * array_info.dim,
                    accessors,
                });
            } else if sequential_indexes_from0 && config.const_generic {
                // Include a ZST ArrayProxy giving indexed access to the
                // elements.
                let ap_path = array_proxy_type(ty, array_info);
                let syn_field =
                    new_syn_field(ty_name.to_snake_case_ident(Span::call_site()), ap_path);
                cluster_expanded.push(RegisterBlockField {
                    syn_field,
                    description: info.description.as_ref().unwrap_or(&info.name).into(),
                    offset: info.address_offset,
                    size: 0,
                    accessors: Vec::new(),
                });
            } else {
                for (field_num, idx) in array_info.indexes().enumerate() {
                    let nb_name = util::replace_suffix(&info.name, &idx);
                    let syn_field =
                        new_syn_field(nb_name.to_snake_case_ident(Span::call_site()), ty.clone());

                    cluster_expanded.push(RegisterBlockField {
                        syn_field,
                        description: description.clone(),
                        offset: info.address_offset + field_num as u32 * array_info.dim_increment,
                        size: cluster_size,
                        accessors: Vec::new(),
                    });
                }
            }
        }
    }

    Ok(cluster_expanded)
}

/// If svd register arrays can't be converted to rust arrays (non sequential addresses, non
/// numeral indexes, or not containing all elements from 0 to size) they will be expanded.
/// A `DeriveInfo::Implicit(_)` will also cause an array to be expanded.
fn expand_register(
    register: &Register,
    derive_info: &DeriveInfo,
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut register_expanded = vec![];

    let register_size = register
        .properties
        .size
        .ok_or_else(|| anyhow!("Register {} has no `size` field", register.name))?;
    let description = register.description.clone().unwrap_or_default();

    let info_name = register.fullname(config.ignore_groups);
    let mut ty_name = if register.is_single() {
        info_name.to_string()
    } else {
        util::replace_suffix(&info_name, "")
    };

    match register {
        Register::Single(info) => {
            let ty = name_to_ty(&ty_name);
            let syn_field = new_syn_field(ty_name.to_snake_case_ident(Span::call_site()), ty);
            register_expanded.push(RegisterBlockField {
                syn_field,
                description,
                offset: info.address_offset,
                size: register_size,
                accessors: Vec::new(),
            })
        }
        Register::Array(info, array_info) => {
            let sequential_addresses = (array_info.dim == 1)
                || (register_size == array_info.dim_increment * BITS_PER_BYTE);

            let convert_list = match config.keep_list {
                true => match &array_info.dim_name {
                    Some(dim_name) => dim_name.contains("[%s]"),
                    None => info.name.contains("[%s]"),
                },
                false => true,
            };

            // if dimIndex exists, test if it is a sequence of numbers from 0 to dim
            let sequential_indexes_from0 = array_info
                .indexes_as_range()
                .filter(|r| *r.start() == 0)
                .is_some();

            // force expansion and rename if we're deriving an array that doesnt start at 0 so we don't get name collisions
            let index: Cow<str> = if let Some(dim_index) = &array_info.dim_index {
                dim_index.first().unwrap().into()
            } else {
                if sequential_indexes_from0 {
                    "0".into()
                } else {
                    "".into()
                }
            };
            let array_convertible = match derive_info {
                DeriveInfo::Implicit(_) => {
                    ty_name = util::replace_suffix(&info_name, &index);
                    sequential_addresses && convert_list && sequential_indexes_from0
                }
                DeriveInfo::Explicit(_) => {
                    ty_name = util::replace_suffix(&info_name, &index);
                    sequential_addresses && convert_list && sequential_indexes_from0
                }
                _ => sequential_addresses && convert_list,
            };
            let ty = name_to_ty(&ty_name);

            if array_convertible {
                let accessors = if sequential_indexes_from0 {
                    Vec::new()
                } else {
                    let span = Span::call_site();
                    let mut accessors = Vec::new();
                    let nb_name_cs = ty_name.to_snake_case_ident(span);
                    for (i, idx) in array_info.indexes().enumerate() {
                        let idx_name =
                            util::replace_suffix(&info_name, &idx).to_snake_case_ident(span);
                        let comment = make_comment(
                            register_size,
                            info.address_offset + (i as u32) * register_size / 8,
                            &description,
                        );
                        let i = unsuffixed(i as _);
                        accessors.push(ArrayAccessor {
                            doc: comment,
                            name: idx_name,
                            ty: ty.clone(),
                            basename: nb_name_cs.clone(),
                            i,
                        });
                    }
                    accessors
                };
                let array_ty = new_syn_array(ty, array_info.dim);
                let syn_field =
                    new_syn_field(ty_name.to_snake_case_ident(Span::call_site()), array_ty);
                register_expanded.push(RegisterBlockField {
                    syn_field,
                    description,
                    offset: info.address_offset,
                    size: register_size * array_info.dim,
                    accessors,
                });
            } else {
                for (field_num, idx) in array_info.indexes().enumerate() {
                    let nb_name = util::replace_suffix(&info_name, &idx);
                    let syn_field =
                        new_syn_field(nb_name.to_snake_case_ident(Span::call_site()), ty.clone());

                    register_expanded.push(RegisterBlockField {
                        syn_field,
                        description: description.clone(),
                        offset: info.address_offset + field_num as u32 * array_info.dim_increment,
                        size: register_size,
                        accessors: Vec::new(),
                    });
                }
            }
        }
    }

    Ok(register_expanded)
}

fn render_ercs(
    ercs: &mut [RegisterCluster],
    derive_infos: &[DeriveInfo],
    path: &BlockPath,
    index: &Index,
    config: &Config,
) -> Result<TokenStream> {
    let mut mod_items = TokenStream::new();

    let zipped = ercs.iter_mut().zip(derive_infos.iter());
    for (erc, derive_info) in zipped {
        match erc {
            // Generate the sub-cluster blocks.
            RegisterCluster::Cluster(c) => {
                trace!("Cluster: {}", c.name);
                let mut cpath = None;
                let dpath = c.derived_from.take();
                if let Some(dpath) = dpath {
                    cpath = derive_cluster(c, &dpath, path, index)?;
                }
                mod_items.extend(cluster_block(c, path, cpath, index, config)?);
            }

            // Generate definition for each of the registers.
            RegisterCluster::Register(reg) => {
                trace!("Register: {}, DeriveInfo: {}", reg.name, derive_info);
                let mut rpath = None;
                if let DeriveInfo::Implicit(rp) = derive_info {
                    rpath = Some(rp.clone());
                } else {
                    let dpath = reg.derived_from.take();
                    if let Some(dpath) = dpath {
                        rpath = derive_register(reg, &dpath, path, index)?;
                    }
                }
                let reg_name = &reg.name;
                let rendered_reg =
                    register::render(reg, path, rpath, index, config).with_context(|| {
                        let descrip = reg.description.as_deref().unwrap_or("No description");
                        format!(
                            "Error rendering register\nName: {reg_name}\nDescription: {descrip}"
                        )
                    })?;
                mod_items.extend(rendered_reg)
            }
        }
    }
    Ok(mod_items)
}

/// Render a Cluster Block into `TokenStream`
fn cluster_block(
    c: &mut Cluster,
    path: &BlockPath,
    dpath: Option<BlockPath>,
    index: &Index,
    config: &Config,
) -> Result<TokenStream> {
    let description =
        util::escape_brackets(&util::respace(c.description.as_ref().unwrap_or(&c.name)));
    let mod_name = util::replace_suffix(&c.name, "");

    // name_snake_case needs to take into account array type.
    let span = Span::call_site();
    let name_snake_case = mod_name.to_snake_case_ident(span);
    let name_constant_case = mod_name.to_constant_case_ident(span);

    if let Some(dpath) = dpath {
        let dparent = dpath.parent().unwrap();
        let mut derived = if &dparent == path {
            type_path(Punctuated::new())
        } else {
            util::block_path_to_ty(&dparent, span)
        };
        let dname = util::replace_suffix(&index.clusters.get(&dpath).unwrap().name, "");
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
            #[doc = #description]
            pub use self::#derived as #name_constant_case;
            pub use self::#mod_derived as #name_snake_case;
        })
    } else {
        let cpath = path.new_cluster(&c.name);
        let mod_derive_infos = check_erc_derive_infos(&mut c.children, &cpath, index, config)?;
        let mod_items = render_ercs(&mut c.children, &mod_derive_infos, &cpath, index, config)?;

        // Generate the register block.
        let cluster_size = match c {
            Cluster::Array(_, array_info) if config.max_cluster_size => {
                Some(array_info.dim_increment)
            }
            _ => None,
        };
        let reg_block = register_or_cluster_block(
            &c.children,
            &mod_derive_infos,
            Some(&mod_name),
            cluster_size,
            config,
        )?;

        let mod_items = quote! {
            #reg_block

            #mod_items
        };

        Ok(quote! {
            #[doc = #description]
            pub use self::#name_snake_case::#name_constant_case;

            ///Cluster
            #[doc = #description]
            pub mod #name_snake_case {
                #mod_items
            }
        })
    }
}

fn new_syn_field(ident: Ident, ty: syn::Type) -> syn::Field {
    let span = Span::call_site();
    syn::Field {
        ident: Some(ident),
        vis: syn::Visibility::Public(syn::VisPublic {
            pub_token: Token![pub](span),
        }),
        attrs: vec![],
        colon_token: Some(Token![:](span)),
        ty,
    }
}

fn new_syn_array(ty: syn::Type, len: u32) -> syn::Type {
    let span = Span::call_site();
    syn::Type::Array(syn::TypeArray {
        bracket_token: syn::token::Bracket { span },
        elem: ty.into(),
        semi_token: Token![;](span),
        len: new_syn_u32(len, span),
    })
}
