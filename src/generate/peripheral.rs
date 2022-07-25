use std::borrow::Cow;
use std::cmp::Ordering;
use svd_parser::expand::{derive_cluster, derive_peripheral, derive_register, BlockPath, Index};

use crate::svd::{array::names, Cluster, ClusterInfo, Peripheral, Register, RegisterCluster};
use log::{debug, trace, warn};
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{parse_str, Token};

use crate::util::{
    self, handle_cluster_error, handle_reg_error, unsuffixed, Config, FullName, ToSanitizedCase,
    BITS_PER_BYTE,
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
    let address = util::hex(p.base_address as u64);
    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));

    let name_snake_case = Ident::new(&name.to_sanitized_snake_case(), span);
    let (derive_regs, base, path) = if let Some(path) = path {
        (
            true,
            Ident::new(&path.peripheral.to_sanitized_snake_case(), span),
            path,
        )
    } else {
        (false, name_snake_case.clone(), BlockPath::new(&p.name))
    };

    let feature_attribute = if config.feature_group && p.group_name.is_some() {
        let feature_name = p.group_name.as_ref().unwrap().to_sanitized_snake_case();
        quote! (#[cfg(feature = #feature_name)])
    } else {
        quote! {}
    };

    match &p {
        Peripheral::Array(p, dim) => {
            let names: Vec<Cow<str>> = names(p, dim).map(|n| n.into()).collect();
            let names_str = names.iter().map(|n| n.to_sanitized_constant_case());
            let names_constant_case = names_str.clone().map(|n| Ident::new(&n, span));
            let addresses =
                (0..=dim.dim).map(|i| util::hex(p.base_address + (i * dim.dim_increment) as u64));

            // Insert the peripherals structure
            out.extend(quote! {
                #(
                    #[doc = #description]
                    #feature_attribute
                    pub struct #names_constant_case { _marker: PhantomData<*const ()> }

                    #feature_attribute
                    unsafe impl Send for #names_constant_case {}

                    #feature_attribute
                    impl #names_constant_case {
                        ///Pointer to the register block
                        pub const PTR: *const #base::RegisterBlock = #addresses as *const _;

                        ///Return the pointer to the register block
                        #[inline(always)]
                        pub const fn ptr() -> *const #base::RegisterBlock {
                            Self::PTR
                        }
                    }

                    #feature_attribute
                    impl Deref for #names_constant_case {
                        type Target = #base::RegisterBlock;

                        #[inline(always)]
                        fn deref(&self) -> &Self::Target {
                            unsafe { &*Self::PTR }
                        }
                    }

                    #feature_attribute
                    impl core::fmt::Debug for #names_constant_case {
                        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                            f.debug_struct(#names_str).finish()
                        }
                    }
                )*
            });
        }
        _ => {
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
        }
    }

    // Derived peripherals may not require re-implementation, and will instead
    // use a single definition of the non-derived version.
    if derive_regs {
        // re-export the base module to allow deriveFrom this one
        out.extend(quote! {
            #[doc = #description]
            #feature_attribute
            pub use #base as #name_snake_case;
        });
        return Ok(out);
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

    debug!("Pushing cluster & register information into output");
    // Push all cluster & register related information into the peripheral module

    let mod_items = render_ercs(&mut ercs, &path, index, config)?;

    // Push any register or cluster blocks into the output
    debug!(
        "Pushing {} register or cluster blocks into output",
        ercs.len()
    );
    let reg_block = register_or_cluster_block(&ercs, None, config)?;

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

#[derive(Clone, Debug)]
struct RegisterBlockField {
    syn_field: syn::Field,
    description: String,
    offset: u32,
    size: u32,
    accessors: Option<TokenStream>,
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
                    "Found type name conflict with region {:?}, renamed to {:?}",
                    r.ident, new_ident
                );
                r.ident = new_ident;
            });
        Ok(())
    }
}

fn make_comment(size: u32, offset: u32, description: &str) -> String {
    if size > 32 {
        format!(
            "0x{:02x}..0x{:02x} - {}",
            offset,
            offset + size / 8,
            util::escape_brackets(&util::respace(description)),
        )
    } else {
        format!(
            "0x{:02x} - {}",
            offset,
            util::escape_brackets(&util::respace(description)),
        )
    }
}

fn register_or_cluster_block(
    ercs: &[RegisterCluster],
    name: Option<&str>,
    config: &Config,
) -> Result<TokenStream> {
    let mut rbfs = TokenStream::new();
    let mut accessors = TokenStream::new();

    let ercs_expanded =
        expand(ercs, name, config).with_context(|| "Could not expand register or cluster block")?;

    // Locate conflicting regions; we'll need to use unions to represent them.
    let mut regions = FieldRegions::default();

    for reg_block_field in &ercs_expanded {
        regions.add(reg_block_field)?;
        if let Some(ts) = &reg_block_field.accessors {
            accessors.extend(ts.clone());
        }
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
            let name = Ident::new(&format!("_reserved{}", i), span);
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
                    pub fn #name(&self) -> &#ty {
                        unsafe {
                            &*(((self as *const Self) as *const u8).add(#offset) as *const #ty)
                        }
                    }
                });
            } else {
                region_rbfs.extend(quote! {
                    #[doc = #comment]
                });

                reg_block_field.syn_field.to_tokens(&mut region_rbfs);
                Punct::new(',', Spacing::Alone).to_tokens(&mut region_rbfs);
            }
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
                    "_reserved_{}_{}",
                    i,
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

    let name = Ident::new(
        &match name {
            Some(name) => name.to_sanitized_constant_case(),
            None => "RegisterBlock".into(),
        },
        span,
    );

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
    name: Option<&str>,
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut ercs_expanded = vec![];

    debug!("Expanding registers or clusters into Register Block Fields");
    for erc in ercs {
        match &erc {
            RegisterCluster::Register(register) => match expand_register(register, name, config) {
                Ok(expanded_reg) => {
                    trace!("Register: {}", register.name);
                    ercs_expanded.extend(expanded_reg);
                }
                Err(e) => {
                    let res = Err(e);
                    return handle_reg_error("Error expanding register", register, res);
                }
            },
            RegisterCluster::Cluster(cluster) => match expand_cluster(cluster, name, config) {
                Ok(expanded_cluster) => {
                    trace!("Cluster: {}", cluster.name);
                    ercs_expanded.extend(expanded_cluster);
                }
                Err(e) => {
                    let res = Err(e);
                    return handle_cluster_error("Error expanding register cluster", cluster, res);
                }
            },
        };
    }

    ercs_expanded.sort_by_key(|x| x.offset);

    Ok(ercs_expanded)
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
                let reg_size: u32 = expand_register(reg, None, config)?
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
fn expand_cluster(
    cluster: &Cluster,
    name: Option<&str>,
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut cluster_expanded = vec![];

    let cluster_size = cluster_info_size_in_bits(cluster, config)
        .with_context(|| format!("Cluster {} has no determinable `size` field", cluster.name))?;

    match cluster {
        Cluster::Single(info) => cluster_expanded.push(RegisterBlockField {
            syn_field: convert_svd_cluster(cluster, name)?,
            description: info.description.as_ref().unwrap_or(&info.name).into(),
            offset: info.address_offset,
            size: cluster_size,
            accessors: None,
        }),
        Cluster::Array(info, array_info) => {
            let sequential_addresses =
                (array_info.dim == 1) || (cluster_size == array_info.dim_increment * BITS_PER_BYTE);

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
                if sequential_indexes_from0 {
                    cluster_expanded.push(RegisterBlockField {
                        syn_field: convert_svd_cluster(cluster, name)?,
                        description: info.description.as_ref().unwrap_or(&info.name).into(),
                        offset: info.address_offset,
                        size: cluster_size * array_info.dim,
                        accessors: None,
                    });
                } else {
                    let span = Span::call_site();
                    let mut accessors = TokenStream::new();
                    let nb_name = util::replace_suffix(&info.name, "");
                    let ty = name_to_ty(&nb_name, name)?;
                    let nb_name_cs = Ident::new(&nb_name.to_sanitized_snake_case(), span);
                    let description = info.description.as_ref().unwrap_or(&info.name);
                    for (i, idx) in array_info.indexes().enumerate() {
                        let idx_name = Ident::new(
                            &util::replace_suffix(&info.name, &idx).to_sanitized_snake_case(),
                            span,
                        );
                        let comment = make_comment(
                            cluster_size,
                            info.address_offset + (i as u32) * cluster_size / 8,
                            description,
                        );
                        let i = unsuffixed(i as _);
                        accessors.extend(quote! {
                            #[doc = #comment]
                            #[inline(always)]
                            pub fn #idx_name(&self) -> &#ty {
                                &self.#nb_name_cs[#i]
                            }
                        });
                    }
                    cluster_expanded.push(RegisterBlockField {
                        syn_field: convert_svd_cluster(cluster, name)?,
                        description: description.into(),
                        offset: info.address_offset,
                        size: cluster_size * array_info.dim,
                        accessors: Some(accessors),
                    });
                }
            } else {
                for (field_num, syn_field) in
                    expand_svd_cluster(cluster, name)?.into_iter().enumerate()
                {
                    cluster_expanded.push(RegisterBlockField {
                        syn_field,
                        description: info.description.as_ref().unwrap_or(&info.name).into(),
                        offset: info.address_offset + field_num as u32 * array_info.dim_increment,
                        size: cluster_size,
                        accessors: None,
                    });
                }
            }
        }
    }

    Ok(cluster_expanded)
}

/// If svd register arrays can't be converted to rust arrays (non sequential addresses, non
/// numeral indexes, or not containing all elements from 0 to size) they will be expanded
fn expand_register(
    register: &Register,
    name: Option<&str>,
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut register_expanded = vec![];

    let register_size = register
        .properties
        .size
        .ok_or_else(|| anyhow!("Register {} has no `size` field", register.name))?;

    match register {
        Register::Single(info) => register_expanded.push(RegisterBlockField {
            syn_field: convert_svd_register(register, name, config.ignore_groups)
                .with_context(|| "syn error occured")?,
            description: info.description.clone().unwrap_or_default(),
            offset: info.address_offset,
            size: register_size,
            accessors: None,
        }),
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

            let array_convertible = sequential_addresses && convert_list;

            if array_convertible {
                // if dimIndex exists, test if it is a sequence of numbers from 0 to dim
                let sequential_indexes_from0 = array_info
                    .indexes_as_range()
                    .filter(|r| *r.start() == 0)
                    .is_some();

                if sequential_indexes_from0 {
                    register_expanded.push(RegisterBlockField {
                        syn_field: convert_svd_register(register, name, config.ignore_groups)?,
                        description: info.description.clone().unwrap_or_default(),
                        offset: info.address_offset,
                        size: register_size * array_info.dim,
                        accessors: None,
                    });
                } else {
                    let span = Span::call_site();
                    let mut accessors = TokenStream::new();
                    let nb_name = util::replace_suffix(&info.fullname(config.ignore_groups), "");
                    let ty = name_to_wrapped_ty(&nb_name, name)?;
                    let nb_name_cs = Ident::new(&nb_name.to_sanitized_snake_case(), span);
                    let description = info.description.clone().unwrap_or_default();
                    let info_name = info.fullname(config.ignore_groups);
                    for (i, idx) in array_info.indexes().enumerate() {
                        let idx_name = Ident::new(
                            &util::replace_suffix(&info_name, &idx).to_sanitized_snake_case(),
                            span,
                        );
                        let comment = make_comment(
                            register_size,
                            info.address_offset + (i as u32) * register_size / 8,
                            &description,
                        );
                        let i = unsuffixed(i as _);
                        accessors.extend(quote! {
                            #[doc = #comment]
                            #[inline(always)]
                            pub fn #idx_name(&self) -> &#ty {
                                &self.#nb_name_cs[#i]
                            }
                        });
                    }
                    register_expanded.push(RegisterBlockField {
                        syn_field: convert_svd_register(register, name, config.ignore_groups)?,
                        description,
                        offset: info.address_offset,
                        size: register_size * array_info.dim,
                        accessors: Some(accessors),
                    });
                }
            } else {
                for (field_num, syn_field) in
                    expand_svd_register(register, name, config.ignore_groups)?
                        .into_iter()
                        .enumerate()
                {
                    register_expanded.push(RegisterBlockField {
                        syn_field,
                        description: info.description.clone().unwrap_or_default(),
                        offset: info.address_offset + field_num as u32 * array_info.dim_increment,
                        size: register_size,
                        accessors: None,
                    });
                }
            }
        }
    }

    Ok(register_expanded)
}

fn render_ercs(
    ercs: &mut [RegisterCluster],
    path: &BlockPath,
    index: &Index,
    config: &Config,
) -> Result<TokenStream> {
    let mut mod_items = TokenStream::new();

    for erc in ercs {
        match erc {
            // Generate the sub-cluster blocks.
            RegisterCluster::Cluster(c) => {
                trace!("Cluster: {}", c.name);
                let mut cpath = None;
                let dpath = c.derived_from.take();
                if let Some(dpath) = dpath {
                    cpath = derive_cluster(c, &dpath, path, index)?;
                }
                let cpath = cpath.unwrap_or_else(|| path.new_cluster(&c.name));
                mod_items.extend(cluster_block(c, &cpath, index, config)?);
            }

            // Generate definition for each of the registers.
            RegisterCluster::Register(reg) => {
                trace!("Register: {}", reg.name);
                let mut rpath = None;
                let dpath = reg.derived_from.take();
                if let Some(dpath) = dpath {
                    rpath = derive_register(reg, &dpath, path, index)?;
                }
                let rpath = rpath.unwrap_or_else(|| path.new_register(&reg.name));
                match register::render(reg, &rpath, index, config) {
                    Ok(rendered_reg) => mod_items.extend(rendered_reg),
                    Err(e) => {
                        let res: Result<TokenStream> = Err(e);
                        return handle_reg_error("Error rendering register", reg, res);
                    }
                };
            }
        }
    }
    Ok(mod_items)
}

/// Render a Cluster Block into `TokenStream`
fn cluster_block(
    c: &mut Cluster,
    path: &BlockPath,
    index: &Index,
    config: &Config,
) -> Result<TokenStream> {
    let mod_items = render_ercs(&mut c.children, path, index, config)?;

    // Generate the register block.
    let mod_name = util::replace_suffix(
        match c {
            Cluster::Single(info) => &info.name,
            Cluster::Array(info, _ai) => &info.name,
        },
        "",
    );

    let reg_block = register_or_cluster_block(&c.children, Some(&mod_name), config)?;

    // name_snake_case needs to take into account array type.
    let description =
        util::escape_brackets(&util::respace(c.description.as_ref().unwrap_or(&c.name)));

    let name_snake_case = Ident::new(&mod_name.to_sanitized_snake_case(), Span::call_site());

    Ok(quote! {
        #reg_block

        ///Register block
        #[doc = #description]
        pub mod #name_snake_case {
            #mod_items
        }
    })
}

/// Takes a svd::Register which may be a register array, and turn in into
/// a list of syn::Field where the register arrays have been expanded.
fn expand_svd_register(
    register: &Register,
    name: Option<&str>,
    ignore_group: bool,
) -> Result<Vec<syn::Field>> {
    if let Register::Array(info, array_info) = register {
        let ty_name = util::replace_suffix(&info.fullname(ignore_group), "");

        let mut out = vec![];
        for idx in array_info.indexes() {
            let nb_name = util::replace_suffix(&info.fullname(ignore_group), &idx);

            let ty = name_to_wrapped_ty(&ty_name, name)?;

            out.push(new_syn_field(&nb_name.to_sanitized_snake_case(), ty));
        }
        Ok(out)
    } else {
        Ok(vec![convert_svd_register(register, name, ignore_group)?])
    }
}

/// Convert a parsed `Register` into its `Field` equivalent
fn convert_svd_register(
    register: &Register,
    name: Option<&str>,
    ignore_group: bool,
) -> Result<syn::Field> {
    Ok(match register {
        Register::Single(info) => {
            let info_name = info.fullname(ignore_group);
            let ty = name_to_wrapped_ty(&info_name, name)
                .with_context(|| format!("Error converting register name {}", info_name))?;
            new_syn_field(&info_name.to_sanitized_snake_case(), ty)
        }
        Register::Array(info, array_info) => {
            let info_name = info.fullname(ignore_group);
            let nb_name = util::replace_suffix(&info_name, "");
            let ty = name_to_wrapped_ty(&nb_name, name)
                .with_context(|| format!("Error converting register name {}", nb_name))?;
            let array_ty = new_syn_array(ty, array_info.dim)?;

            new_syn_field(&nb_name.to_sanitized_snake_case(), array_ty)
        }
    })
}

/// Takes a svd::Cluster which may contain a register array, and turn it into
/// a list of syn::Field where the register arrays have been expanded.
fn expand_svd_cluster(
    cluster: &Cluster,
    name: Option<&str>,
) -> Result<Vec<syn::Field>, syn::Error> {
    if let Cluster::Array(info, array_info) = cluster {
        let ty_name = util::replace_suffix(&info.name, "");

        let mut out = vec![];
        for idx in array_info.indexes() {
            let nb_name = util::replace_suffix(&info.name, &idx);

            let ty = name_to_ty(&ty_name, name)?;

            out.push(new_syn_field(&nb_name.to_sanitized_snake_case(), ty));
        }
        Ok(out)
    } else {
        Ok(vec![convert_svd_cluster(cluster, name)?])
    }
}

/// Convert a parsed `Cluster` into its `Field` equivalent
fn convert_svd_cluster(cluster: &Cluster, name: Option<&str>) -> Result<syn::Field, syn::Error> {
    Ok(match cluster {
        Cluster::Single(info) => {
            let ty_name = util::replace_suffix(&info.name, "");
            let ty = name_to_ty(&ty_name, name)?;
            new_syn_field(&info.name.to_sanitized_snake_case(), ty)
        }
        Cluster::Array(info, array_info) => {
            let ty_name = util::replace_suffix(&info.name, "");
            let ty = name_to_ty(&ty_name, name)?;
            let array_ty = new_syn_array(ty, array_info.dim)?;

            new_syn_field(&ty_name.to_sanitized_snake_case(), array_ty)
        }
    })
}

fn new_syn_field(ident: &str, ty: syn::Type) -> syn::Field {
    let span = Span::call_site();
    syn::Field {
        ident: Some(Ident::new(ident, span)),
        vis: syn::Visibility::Public(syn::VisPublic {
            pub_token: Token![pub](span),
        }),
        attrs: vec![],
        colon_token: Some(Token![:](span)),
        ty,
    }
}

fn new_syn_array(ty: syn::Type, len: u32) -> Result<syn::Type, syn::Error> {
    let span = Span::call_site();
    Ok(syn::Type::Array(syn::TypeArray {
        bracket_token: syn::token::Bracket { span },
        elem: ty.into(),
        semi_token: Token![;](span),
        len: new_syn_u32(len, span),
    }))
}

fn new_syn_u32(len: u32, span: Span) -> syn::Expr {
    syn::Expr::Lit(syn::ExprLit {
        attrs: Vec::new(),
        lit: syn::Lit::Int(syn::LitInt::new(&len.to_string(), span)),
    })
}

fn name_to_ty_str<'a, 'b>(name: &'a str, ns: Option<&'b str>) -> Cow<'a, str> {
    if let Some(ns) = ns {
        Cow::Owned(
            String::from("self::")
                + &ns.to_sanitized_snake_case()
                + "::"
                + &name.to_sanitized_constant_case(),
        )
    } else {
        name.to_sanitized_constant_case()
    }
}

fn name_to_ty(name: &str, ns: Option<&str>) -> Result<syn::Type, syn::Error> {
    let ident = name_to_ty_str(name, ns);
    parse_str::<syn::TypePath>(&ident).map(syn::Type::Path)
}

fn name_to_wrapped_ty_str(name: &str, ns: Option<&str>) -> String {
    if let Some(ns) = ns {
        format!(
            "crate::Reg<self::{}::{}::{}_SPEC>",
            &ns.to_sanitized_snake_case(),
            &name.to_sanitized_snake_case(),
            &name.to_sanitized_constant_case(),
        )
    } else {
        format!(
            "crate::Reg<{}::{}_SPEC>",
            &name.to_sanitized_snake_case(),
            &name.to_sanitized_constant_case(),
        )
    }
}

fn name_to_wrapped_ty(name: &str, ns: Option<&str>) -> Result<syn::Type> {
    let ident = name_to_wrapped_ty_str(name, ns);
    parse_str::<syn::TypePath>(&ident)
        .map(syn::Type::Path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Determining syn::TypePath from ident \"{}\" failed", ident))
}
