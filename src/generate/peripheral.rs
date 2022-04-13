use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::svd::{
    Cluster, ClusterInfo, DeriveFrom, DimElement, Peripheral, Register, RegisterCluster,
    RegisterProperties,
};
use log::{debug, trace, warn};
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{parse_str, Token};

use crate::util::{
    self, handle_cluster_error, handle_reg_error, unsuffixed, Config, FullName,
    ToSanitizedSnakeCase, ToSanitizedUpperCase, BITS_PER_BYTE,
};
use anyhow::{anyhow, bail, Context, Result};

use crate::generate::register;

pub fn render(
    p_original: &Peripheral,
    all_peripherals: &[Peripheral],
    defaults: &RegisterProperties,
    config: &Config,
) -> Result<TokenStream> {
    let mut out = TokenStream::new();

    let p_derivedfrom = p_original
        .derived_from
        .as_ref()
        .and_then(|s| all_peripherals.iter().find(|x| x.name == *s));

    let p_merged = p_derivedfrom.map(|ancestor| p_original.derive_from(ancestor));
    let p = p_merged.as_ref().unwrap_or(p_original);

    if let (Some(df), None) = (p_original.derived_from.as_ref(), &p_derivedfrom) {
        eprintln!(
            "Couldn't find derivedFrom original: {} for {}, skipping",
            df, p_original.name
        );
        return Ok(out);
    }

    let span = Span::call_site();
    let name_str = p.name.to_sanitized_upper_case();
    let name_pc = Ident::new(&name_str, span);
    let address = util::hex(p.base_address as u64);
    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));

    let name_sc = Ident::new(&p.name.to_sanitized_snake_case(), span);
    let (derive_regs, base) = if let (Some(df), None) = (p_derivedfrom, &p_original.registers) {
        (true, Ident::new(&df.name.to_sanitized_snake_case(), span))
    } else {
        (false, name_sc.clone())
    };

    // Insert the peripheral structure
    out.extend(quote! {
        #[doc = #description]
        pub struct #name_pc { _marker: PhantomData<*const ()> }

        unsafe impl Send for #name_pc {}

        impl #name_pc {
            ///Pointer to the register block
            pub const PTR: *const #base::RegisterBlock = #address as *const _;

            ///Return the pointer to the register block
            #[inline(always)]
            pub const fn ptr() -> *const #base::RegisterBlock {
                Self::PTR
            }
        }

        impl Deref for #name_pc {
            type Target = #base::RegisterBlock;

            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                unsafe { &*Self::PTR }
            }
        }

        impl core::fmt::Debug for #name_pc {
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
            pub use #base as #name_sc;
        });
        return Ok(out);
    }

    // erc: *E*ither *R*egister or *C*luster
    let ercs = p.registers.as_ref().map(|x| x.as_ref()).unwrap_or(&[][..]);

    // make a pass to expand derived registers and clusters.  Ideally, for the most minimal
    // code size, we'd do some analysis to figure out if we can 100% reuse the
    // code that we're deriving from.  For the sake of proving the concept, we're
    // just going to emit a second copy of the accessor code.  It'll probably
    // get inlined by the compiler anyway, right? :-)

    // Build a map so that we can look up registers within this peripheral
    let mut erc_map = HashMap::new();
    for erc in ercs {
        erc_map.insert(util::erc_name(erc), erc.clone());
    }

    // Build up an alternate erc list by expanding any derived registers/clusters
    let ercs: Vec<RegisterCluster> = ercs
        .iter()
        .filter_map(|erc| match util::erc_derived_from(erc) {
            Some(ref derived) => {
                let ancestor = match erc_map.get(derived) {
                    Some(erc) => erc,
                    None => {
                        eprintln!(
                            "register/cluster {} derivedFrom missing register/cluster {}",
                            util::erc_name(erc),
                            derived
                        );
                        return None;
                    }
                };

                match (erc, ancestor) {
                    (RegisterCluster::Register(reg), RegisterCluster::Register(other_reg)) => {
                        Some(RegisterCluster::Register(reg.derive_from(other_reg)))
                    }
                    (
                        RegisterCluster::Cluster(cluster),
                        RegisterCluster::Cluster(other_cluster),
                    ) => Some(RegisterCluster::Cluster(cluster.derive_from(other_cluster))),
                    _ => {
                        eprintln!(
                            "{} can't derive from {}",
                            util::erc_name(erc),
                            util::erc_name(ancestor)
                        );
                        None
                    }
                }
            }
            None => Some(erc.clone()),
        })
        .collect();

    // And revise registers, clusters and ercs to refer to our expanded versions
    let registers: &[&Register] = &util::only_registers(&ercs)[..];
    let clusters = util::only_clusters(&ercs);

    // No `struct RegisterBlock` can be generated
    if registers.is_empty() && clusters.is_empty() {
        // Drop the definition of the peripheral
        return Ok(TokenStream::new());
    }

    let defaults = p.default_register_properties.derive_from(defaults);

    // Push any register or cluster blocks into the output
    debug!(
        "Pushing {} register or cluster blocks into output",
        ercs.len()
    );
    let mut mod_items = TokenStream::new();
    mod_items.extend(register_or_cluster_block(&ercs, &defaults, None, config)?);

    debug!("Pushing cluster information into output");
    // Push all cluster related information into the peripheral module
    for c in &clusters {
        trace!("Cluster: {}", c.name);
        mod_items.extend(cluster_block(c, &defaults, p, all_peripherals, config)?);
    }

    debug!("Pushing register information into output");
    // Push all register related information into the peripheral module
    for reg in registers {
        trace!("Register: {}", reg.name);
        match register::render(reg, registers, p, all_peripherals, &defaults, config) {
            Ok(rendered_reg) => mod_items.extend(rendered_reg),
            Err(e) => {
                let res: Result<TokenStream> = Err(e);
                return handle_reg_error("Error rendering register", *reg, res);
            }
        };
    }

    let description =
        util::escape_brackets(util::respace(p.description.as_ref().unwrap_or(&p.name)).as_ref());

    let open = Punct::new('{', Spacing::Alone);
    let close = Punct::new('}', Spacing::Alone);

    out.extend(quote! {
        #[doc = #description]
        pub mod #name_sc #open
    });

    out.extend(mod_items);

    close.to_tokens(&mut out);

    Ok(out)
}

#[derive(Clone, Debug)]
struct RegisterBlockField {
    field: syn::Field,
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
            .filter_map(|f| f.field.ident.as_ref().map(|ident| ident.to_string()))
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
            .filter_map(|f| f.field.ident.as_ref().map(|ident| ident.to_string()))
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
    defs: &RegisterProperties,
    name: Option<&str>,
    config: &Config,
) -> Result<TokenStream> {
    let mut rbfs = TokenStream::new();
    let mut accessors = TokenStream::new();

    let ercs_expanded = expand(ercs, defs, name, config)
        .with_context(|| "Could not expand register or cluster block")?;

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
                let name = &reg_block_field.field.ident;
                let ty = &reg_block_field.field.ty;
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

                reg_block_field.field.to_tokens(&mut region_rbfs);
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
            Some(name) => name.to_sanitized_upper_case(),
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
    defs: &RegisterProperties,
    name: Option<&str>,
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut ercs_expanded = vec![];

    debug!("Expanding registers or clusters into Register Block Fields");
    for erc in ercs {
        match &erc {
            RegisterCluster::Register(register) => {
                match expand_register(register, defs, name, config) {
                    Ok(expanded_reg) => {
                        trace!("Register: {}", register.name);
                        ercs_expanded.extend(expanded_reg);
                    }
                    Err(e) => {
                        let res = Err(e);
                        return handle_reg_error("Error expanding register", register, res);
                    }
                }
            }
            RegisterCluster::Cluster(cluster) => {
                match expand_cluster(cluster, defs, name, config) {
                    Ok(expanded_cluster) => {
                        trace!("Cluster: {}", cluster.name);
                        ercs_expanded.extend(expanded_cluster);
                    }
                    Err(e) => {
                        let res = Err(e);
                        return handle_cluster_error(
                            "Error expanding register cluster",
                            cluster,
                            res,
                        );
                    }
                }
            }
        };
    }

    ercs_expanded.sort_by_key(|x| x.offset);

    Ok(ercs_expanded)
}

/// Calculate the size of a Cluster.  If it is an array, then the dimensions
/// tell us the size of the array.  Otherwise, inspect the contents using
/// [cluster_info_size_in_bits].
fn cluster_size_in_bits(
    cluster: &Cluster,
    defs: &RegisterProperties,
    config: &Config,
) -> Result<u32> {
    match cluster {
        Cluster::Single(info) => cluster_info_size_in_bits(info, defs, config),
        // If the contained array cluster has a mismatch between the
        // dimIncrement and the size of the array items, then the array
        // will get expanded in expand_cluster below.  The overall size
        // then ends at the last array entry.
        Cluster::Array(info, dim) => {
            if dim.dim == 0 {
                return Ok(0); // Special case!
            }
            let last_offset = (dim.dim - 1) * dim.dim_increment * BITS_PER_BYTE;
            let last_size = cluster_info_size_in_bits(info, defs, config);
            Ok(last_offset + last_size?)
        }
    }
}

/// Recursively calculate the size of a ClusterInfo. A cluster's size is the
/// maximum end position of its recursive children.
fn cluster_info_size_in_bits(
    info: &ClusterInfo,
    defs: &RegisterProperties,
    config: &Config,
) -> Result<u32> {
    let mut size = 0;

    for c in &info.children {
        let end = match c {
            RegisterCluster::Register(reg) => {
                let reg_size: u32 = expand_register(reg, defs, None, config)?
                    .iter()
                    .map(|rbf| rbf.size)
                    .sum();

                (reg.address_offset * BITS_PER_BYTE) + reg_size
            }
            RegisterCluster::Cluster(clust) => {
                (clust.address_offset * BITS_PER_BYTE) + cluster_size_in_bits(clust, defs, config)?
            }
        };

        size = size.max(end);
    }
    Ok(size)
}

/// Render a given cluster (and any children) into `RegisterBlockField`s
fn expand_cluster(
    cluster: &Cluster,
    defs: &RegisterProperties,
    name: Option<&str>,
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut cluster_expanded = vec![];

    let defs = cluster.default_register_properties.derive_from(defs);

    let cluster_size = cluster_info_size_in_bits(cluster, &defs, config)
        .with_context(|| format!("Cluster {} has no determinable `size` field", cluster.name))?;

    match cluster {
        Cluster::Single(info) => cluster_expanded.push(RegisterBlockField {
            field: convert_svd_cluster(cluster, name)?,
            description: info.description.as_ref().unwrap_or(&info.name).into(),
            offset: info.address_offset,
            size: cluster_size,
            accessors: None,
        }),
        Cluster::Array(info, array_info) => {
            let sequential_addresses =
                (array_info.dim == 1) || (cluster_size == array_info.dim_increment * BITS_PER_BYTE);

            // if dimIndex exists, test if it is a sequence of numbers from 0 to dim
            let sequential_indexes = array_info.dim_index.as_ref().map_or(true, |dim_index| {
                dim_index
                    .iter()
                    .map(|element| element.parse::<u32>())
                    .eq((0..array_info.dim).map(Ok))
            });

            let convert_list = match config.keep_list {
                true => match &array_info.dim_name {
                    Some(dim_name) => dim_name.contains("[%s]"),
                    None => info.name.contains("[%s]"),
                },
                false => true,
            };

            let array_convertible = sequential_addresses && convert_list;

            if array_convertible {
                if sequential_indexes {
                    cluster_expanded.push(RegisterBlockField {
                        field: convert_svd_cluster(cluster, name)?,
                        description: info.description.as_ref().unwrap_or(&info.name).into(),
                        offset: info.address_offset,
                        size: cluster_size * array_info.dim,
                        accessors: None,
                    });
                } else {
                    let mut accessors = TokenStream::new();
                    let nb_name = util::replace_suffix(&info.name, "");
                    let ty = name_to_ty(&nb_name, name)?;
                    let nb_name_cs =
                        Ident::new(&nb_name.to_sanitized_snake_case(), Span::call_site());
                    let description = info.description.as_ref().unwrap_or(&info.name);
                    for (i, idx) in array_info.indexes().enumerate() {
                        let idx_name = Ident::new(
                            &util::replace_suffix(&info.name, &idx).to_sanitized_snake_case(),
                            Span::call_site(),
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
                        field: convert_svd_cluster(cluster, name)?,
                        description: description.into(),
                        offset: info.address_offset,
                        size: cluster_size * array_info.dim,
                        accessors: Some(accessors),
                    });
                }
            } else if sequential_indexes && config.const_generic {
                // Include a ZST ArrayProxy giving indexed access to the
                // elements.
                cluster_expanded.push(array_proxy(info, array_info, name)?);
            } else {
                for (field_num, field) in expand_svd_cluster(cluster, name)?.iter().enumerate() {
                    cluster_expanded.push(RegisterBlockField {
                        field: field.clone(),
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
    defs: &RegisterProperties,
    name: Option<&str>,
    config: &Config,
) -> Result<Vec<RegisterBlockField>> {
    let mut register_expanded = vec![];

    let register_size = register
        .properties
        .size
        .or(defs.size)
        .ok_or_else(|| anyhow!("Register {} has no `size` field", register.name))?;

    match register {
        Register::Single(info) => register_expanded.push(RegisterBlockField {
            field: convert_svd_register(register, name, config.ignore_groups)
                .with_context(|| "syn error occured")?,
            description: info.description.clone().unwrap_or_default(),
            offset: info.address_offset,
            size: register_size,
            accessors: None,
        }),
        Register::Array(info, array_info) => {
            let sequential_addresses = (array_info.dim == 1)
                || (register_size == array_info.dim_increment * BITS_PER_BYTE);

            // if dimIndex exists, test if it is a sequence of numbers from 0 to dim
            let sequential_indexes = array_info.dim_index.as_ref().map_or(true, |dim_index| {
                dim_index
                    .iter()
                    .map(|element| element.parse::<u32>())
                    .eq((0..array_info.dim).map(Ok))
            });

            let convert_list = match config.keep_list {
                true => match &array_info.dim_name {
                    Some(dim_name) => dim_name.contains("[%s]"),
                    None => info.name.contains("[%s]"),
                },
                false => true,
            };

            let array_convertible = sequential_addresses && convert_list;

            if array_convertible {
                if sequential_indexes {
                    register_expanded.push(RegisterBlockField {
                        field: convert_svd_register(register, name, config.ignore_groups)?,
                        description: info.description.clone().unwrap_or_default(),
                        offset: info.address_offset,
                        size: register_size * array_info.dim,
                        accessors: None,
                    });
                } else {
                    let mut accessors = TokenStream::new();
                    let nb_name = util::replace_suffix(&info.fullname(config.ignore_groups), "");
                    let ty = name_to_wrapped_ty(&nb_name, name)?;
                    let nb_name_cs =
                        Ident::new(&nb_name.to_sanitized_snake_case(), Span::call_site());
                    let description = info.description.clone().unwrap_or_default();
                    for (i, idx) in array_info.indexes().enumerate() {
                        let idx_name = Ident::new(
                            &util::replace_suffix(&info.fullname(config.ignore_groups), &idx)
                                .to_sanitized_snake_case(),
                            Span::call_site(),
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
                        field: convert_svd_register(register, name, config.ignore_groups)?,
                        description,
                        offset: info.address_offset,
                        size: register_size * array_info.dim,
                        accessors: Some(accessors),
                    });
                }
            } else {
                for (field_num, field) in expand_svd_register(register, name, config.ignore_groups)?
                    .iter()
                    .enumerate()
                {
                    register_expanded.push(RegisterBlockField {
                        field: field.clone(),
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

/// Render a Cluster Block into `TokenStream`
fn cluster_block(
    c: &Cluster,
    defaults: &RegisterProperties,
    p: &Peripheral,
    all_peripherals: &[Peripheral],
    config: &Config,
) -> Result<TokenStream> {
    let mut mod_items = TokenStream::new();

    // name_sc needs to take into account array type.
    let description =
        util::escape_brackets(util::respace(c.description.as_ref().unwrap_or(&c.name)).as_ref());

    // Generate the register block.
    let mod_name = util::replace_suffix(
        match c {
            Cluster::Single(info) => &info.name,
            Cluster::Array(info, _ai) => &info.name,
        },
        "",
    );
    let name_sc = Ident::new(&mod_name.to_sanitized_snake_case(), Span::call_site());

    let defaults = c.default_register_properties.derive_from(defaults);

    let reg_block = register_or_cluster_block(&c.children, &defaults, Some(&mod_name), config)?;

    // Generate definition for each of the registers.
    let registers = util::only_registers(&c.children);
    for reg in &registers {
        match register::render(reg, &registers, p, all_peripherals, &defaults, config) {
            Ok(rendered_reg) => mod_items.extend(rendered_reg),
            Err(e) => {
                let res: Result<TokenStream> = Err(e);
                return handle_reg_error(
                    "Error generating register definition for a register cluster",
                    *reg,
                    res,
                );
            }
        };
    }

    // Generate the sub-cluster blocks.
    let clusters = util::only_clusters(&c.children);
    for c in &clusters {
        mod_items.extend(cluster_block(c, &defaults, p, all_peripherals, config)?);
    }

    Ok(quote! {
        #reg_block

        ///Register block
        #[doc = #description]
        pub mod #name_sc {
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
    let mut out = vec![];

    match register {
        Register::Single(_info) => out.push(convert_svd_register(register, name, ignore_group)?),
        Register::Array(info, array_info) => {
            let ty_name = util::replace_suffix(&info.fullname(ignore_group), "");

            for idx in array_info.indexes() {
                let nb_name = util::replace_suffix(&info.fullname(ignore_group), &idx);

                let ty = name_to_wrapped_ty(&ty_name, name)?;

                out.push(new_syn_field(&nb_name.to_sanitized_snake_case(), ty));
            }
        }
    }
    Ok(out)
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
            new_syn_field(
                &info_name.to_sanitized_snake_case(),
                name_to_wrapped_ty(&info_name, name)
                    .with_context(|| format!("Error converting info name {}", info_name))?,
            )
        }
        Register::Array(info, array_info) => {
            let nb_name = util::replace_suffix(&info.fullname(ignore_group), "");
            let ty = syn::Type::Array(parse_str::<syn::TypeArray>(&format!(
                "[{};{}]",
                name_to_wrapped_ty_str(&nb_name, name),
                u64::from(array_info.dim)
            ))?);

            new_syn_field(&nb_name.to_sanitized_snake_case(), ty)
        }
    })
}

/// Return an syn::Type for an ArrayProxy.
fn array_proxy(
    info: &ClusterInfo,
    array_info: &DimElement,
    name: Option<&str>,
) -> Result<RegisterBlockField, syn::Error> {
    let ty_name = util::replace_suffix(&info.name, "");
    let tys = name_to_ty_str(&ty_name, name);

    let ap_path = parse_str::<syn::TypePath>(&format!(
        "crate::ArrayProxy<{}, {}, {}>",
        tys,
        array_info.dim,
        util::hex(array_info.dim_increment as u64)
    ))?;

    Ok(RegisterBlockField {
        field: new_syn_field(&ty_name.to_sanitized_snake_case(), ap_path.into()),
        description: info.description.as_ref().unwrap_or(&info.name).into(),
        offset: info.address_offset,
        size: 0,
        accessors: None,
    })
}

/// Takes a svd::Cluster which may contain a register array, and turn in into
/// a list of syn::Field where the register arrays have been expanded.
fn expand_svd_cluster(
    cluster: &Cluster,
    name: Option<&str>,
) -> Result<Vec<syn::Field>, syn::Error> {
    let mut out = vec![];

    match &cluster {
        Cluster::Single(_info) => out.push(convert_svd_cluster(cluster, name)?),
        Cluster::Array(info, array_info) => {
            let ty_name = util::replace_suffix(&info.name, "");

            for idx in array_info.indexes() {
                let nb_name = util::replace_suffix(&info.name, &idx);

                let ty = name_to_ty(&ty_name, name)?;

                out.push(new_syn_field(&nb_name.to_sanitized_snake_case(), ty));
            }
        }
    }
    Ok(out)
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

            let ty = syn::Type::Array(parse_str::<syn::TypeArray>(&format!(
                "[{};{}]",
                name_to_ty_str(&ty_name, name),
                u64::from(array_info.dim)
            ))?);

            new_syn_field(&ty_name.to_sanitized_snake_case(), ty)
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

fn name_to_ty_str<'a, 'b>(name: &'a str, ns: Option<&'b str>) -> Cow<'a, str> {
    if let Some(ns) = ns {
        Cow::Owned(
            String::from("self::")
                + &ns.to_sanitized_snake_case()
                + "::"
                + &name.to_sanitized_upper_case(),
        )
    } else {
        name.to_sanitized_upper_case()
    }
}

fn name_to_ty(name: &str, ns: Option<&str>) -> Result<syn::Type, syn::Error> {
    let ident = name_to_ty_str(name, ns);
    Ok(syn::Type::Path(parse_str::<syn::TypePath>(&ident)?))
}

fn name_to_wrapped_ty_str(name: &str, ns: Option<&str>) -> String {
    if let Some(ns) = ns {
        format!(
            "crate::Reg<self::{}::{}::{}_SPEC>",
            &ns.to_sanitized_snake_case(),
            &name.to_sanitized_snake_case(),
            &name.to_sanitized_upper_case(),
        )
    } else {
        format!(
            "crate::Reg<{}::{}_SPEC>",
            &name.to_sanitized_snake_case(),
            &name.to_sanitized_upper_case(),
        )
    }
}

fn name_to_wrapped_ty(name: &str, ns: Option<&str>) -> Result<syn::Type> {
    let ident = name_to_wrapped_ty_str(name, ns);
    match parse_str::<syn::TypePath>(&ident) {
        Ok(path) => Ok(syn::Type::Path(path)),
        Err(e) => {
            let mut res = Err(e.into());
            res = res.with_context(|| {
                format!("Determining syn::TypePath from ident \"{}\" failed", ident)
            });
            res
        }
    }
}
