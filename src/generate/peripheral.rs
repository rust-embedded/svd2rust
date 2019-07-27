use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;

use quote::{ToTokens, Tokens};
use crate::svd::{Cluster, ClusterInfo, Defaults, Peripheral, Register, RegisterCluster, RegisterInfo};
use syn::{self, Ident};
use log::warn;

use crate::errors::*;
use crate::util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase, BITS_PER_BYTE};

use crate::generate::register;

pub fn render(
    p_original: &Peripheral,
    all_peripherals: &[Peripheral],
    defaults: &Defaults,
    nightly: bool,
) -> Result<Vec<Tokens>> {
    let mut out = vec![];

    let p_derivedfrom = p_original.derived_from.as_ref().and_then(|s| {
        all_peripherals.iter().find(|x| x.name == *s)
    });

    let p_merged = p_derivedfrom.map(|ancestor| p_original.derive_from(ancestor));
    let p = p_merged.as_ref().unwrap_or(p_original);

    if p_original.derived_from.is_some() && p_derivedfrom.is_none() {
        eprintln!("Couldn't find derivedFrom original: {} for {}, skipping",
            p_original.derived_from.as_ref().unwrap(), p_original.name);
        return Ok(out);
    }

    let name_pc = Ident::from(&*p.name.to_sanitized_upper_case());
    let address = util::hex(p.base_address as u64);
    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));
    let derive_regs = p_derivedfrom.is_some() && p_original.registers.is_none();

    let name_sc = Ident::from(&*p.name.to_sanitized_snake_case());
    let base = if derive_regs {
        Ident::from(&*p_derivedfrom.unwrap().name.to_sanitized_snake_case())
    } else {
        name_sc.clone()
    };

    // Insert the peripheral structure
    out.push(quote! {
        #[doc = #description]
        pub struct #name_pc { _marker: PhantomData<*const ()> }

        unsafe impl Send for #name_pc {}

        impl #name_pc {
            ///Returns a pointer to the register block
            #[inline(always)]
            pub const fn ptr() -> *const #base::RegisterBlock {
                #address as *const _
            }
        }

        impl Deref for #name_pc {
            type Target = #base::RegisterBlock;

            fn deref(&self) -> &Self::Target {
                unsafe { &*#name_pc::ptr() }
            }
        }
    });

    // Derived peripherals may not require re-implementation, and will instead
    // use a single definition of the non-derived version.
    if derive_regs {
        return Ok(out);
    }

    // erc: *E*ither *R*egister or *C*luster
    let ercs = p.registers.as_ref().map(|x| x.as_ref()).unwrap_or(&[][..]);
    let registers: &[&Register] = &util::only_registers(&ercs)[..];

    // make a pass to expand derived registers.  Ideally, for the most minimal
    // code size, we'd do some analysis to figure out if we can 100% reuse the
    // code that we're deriving from.  For the sake of proving the concept, we're
    // just going to emit a second copy of the accessor code.  It'll probably
    // get inlined by the compiler anyway, right? :-)

    // Build a map so that we can look up registers within this peripheral
    let mut reg_map = HashMap::new();
    for r in registers {
        reg_map.insert(&r.name, r.clone());
    }

    // Compute the effective, derived version of a register given the definition
    // with the derived_from property on it (`info`) and its `ancestor`
    fn derive_reg_info(info: &RegisterInfo, ancestor: &RegisterInfo) -> RegisterInfo {
        let mut derived = info.clone();

        if derived.size.is_none() {
            derived.size = ancestor.size.clone();
        }
        if derived.access.is_none() {
            derived.access = ancestor.access.clone();
        }
        if derived.reset_value.is_none() {
            derived.reset_value = ancestor.reset_value.clone();
        }
        if derived.reset_mask.is_none() {
            derived.reset_mask = ancestor.reset_mask.clone();
        }
        if derived.fields.is_none() {
            derived.fields = ancestor.fields.clone();
        }
        if derived.write_constraint.is_none() {
            derived.write_constraint = ancestor.write_constraint.clone();
        }

        derived
    }

    // Build up an alternate erc list by expanding any derived registers
    let mut alt_erc :Vec<RegisterCluster> = registers.iter().filter_map(|r| {
        match r.derived_from {
            Some(ref derived) => {
                let ancestor = match reg_map.get(derived) {
                    Some(r) => r,
                    None => {
                        eprintln!("register {} derivedFrom missing register {}", r.name, derived);
                        return None
                    }
                };

                let d = match **ancestor {
                    Register::Array(ref info, ref array_info) => {
                        Some(RegisterCluster::Register(Register::Array(derive_reg_info(*r, info), array_info.clone())))
                    }
                    Register::Single(ref info) => {
                        Some(RegisterCluster::Register(Register::Single(derive_reg_info(*r, info))))
                    }
                };

                d
            }
            None => Some(RegisterCluster::Register((*r).clone())),
        }
    }).collect();

    // Now add the clusters to our alternate erc list
    let clusters = util::only_clusters(ercs);
    for cluster in &clusters {
        alt_erc.push(RegisterCluster::Cluster((*cluster).clone()));
    }

    // And revise registers, clusters and ercs to refer to our expanded versions
    let registers: &[&Register] = &util::only_registers(&alt_erc)[..];
    let clusters = util::only_clusters(ercs);
    let ercs = &alt_erc;

    // No `struct RegisterBlock` can be generated
    if registers.is_empty() && clusters.is_empty() {
        // Drop the definition of the peripheral
        out.pop();
        return Ok(out);
    }

    // Push any register or cluster blocks into the output
    let mut mod_items = vec![];
    mod_items.push(register_or_cluster_block(ercs, defaults, None, nightly)?);

    // Push all cluster related information into the peripheral module
    for c in &clusters {
        mod_items.push(cluster_block(c, defaults, p, all_peripherals, nightly)?);
    }

    // Push all regsiter realted information into the peripheral module
    for reg in registers {
        mod_items.extend(register::render(
            reg,
            registers,
            p,
            all_peripherals,
            defaults,
        )?);
    }

    let description =
        util::escape_brackets(util::respace(p.description.as_ref().unwrap_or(&p.name)).as_ref());

    let open = Ident::from("{");
    let close = Ident::from("}");

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

#[derive(Clone, Debug)]
struct RegisterBlockField {
    field: syn::Field,
    description: String,
    offset: u32,
    size: u32,
}

#[derive(Clone, Debug)]
struct Region {
    fields: Vec<RegisterBlockField>,
    offset: u32,
    end: u32,
    /// This is only used for regions with `fields.len() > 1`
    pub ident: Option<String>,
}

impl Region {
    fn shortest_ident(&self) -> Option<String> {
        let mut idents: Vec<_> = self
            .fields
            .iter()
            .filter_map(|f| match &f.field.ident {
                None => None,
                Some(ident) => Some(ident.as_ref()),
            })
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
            .fields
            .iter()
            .filter_map(|f| match &f.field.ident {
                None => None,
                Some(ident) => Some(ident.as_ref()),
            })
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
        } else if first.get(index).is_some() && first[index].chars().all(|c| c.is_numeric()) {
            Some(first.iter().take(index).cloned().collect())
        } else {
            Some(first.iter().take(index - 1).cloned().collect())
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
        self.fields.len() > 1
    }
}

/// FieldRegions keeps track of overlapping field regions,
/// merging fields into appropriate regions as we process them.
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
    fn add(&mut self, field: &RegisterBlockField) -> Result<()> {
        // When merging, this holds the indices in self.regions
        // that the input `field` will be merging with.
        let mut indices = Vec::new();

        let field_start = field.offset;
        let field_end = field_start + field.size / BITS_PER_BYTE;

        // The region that we're going to insert
        let mut new_region = Region {
            fields: vec![field.clone()],
            offset: field.offset,
            end: field.offset + field.size / BITS_PER_BYTE,
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
            let begin = f_start.max(field_start);
            let end = f_end.min(field_end);

            if end > begin {
                // We're going to remove this element and fold it
                // into our new region
                indices.push(idx);

                // Expand the existing entry
                new_region.offset = new_region.offset.min(f_start);
                new_region.end = new_region.end.max(f_end);

                // And merge in the fields
                new_region.fields.append(&mut f.fields);
            }
        }

        // Now remove the entries that we collapsed together.
        // We do this in reverse order to ensure that the indices
        // are stable in the face of removal.
        for idx in indices.iter().rev() {
            self.regions.remove(*idx);
        }

        new_region.fields.sort_by_key(|f| f.offset);

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
                .filter(|r| r.fields.len() > 1)
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
                r.fields.len() > 1 && (idents.iter().filter(|ident| **ident == r.ident).count() > 1)
            })
            .inspect(|r| {
                warn!(
                    "Found type name conflict with region {:?}, renamed to {:?}",
                    r.ident,
                    r.shortest_ident()
                )
            })
            .for_each(|r| {
                r.ident = r.shortest_ident();
            });
        Ok(())
    }
}

fn register_or_cluster_block(
    ercs: &[RegisterCluster],
    defs: &Defaults,
    name: Option<&str>,
    _nightly: bool,
) -> Result<Tokens> {
    let mut fields = Tokens::new();
    let mut accessors = Tokens::new();
    let mut have_accessors = false;

    let ercs_expanded = expand(ercs, defs, name)?;

    // Locate conflicting regions; we'll need to use unions to represent them.
    let mut regions = FieldRegions::default();

    for reg_block_field in &ercs_expanded {
        regions.add(reg_block_field)?;
    }

    // We need to compute the idents of each register/union block first to make sure no conflicts exists.
    regions.resolve_idents()?;
    // The end of the region for which we previously emitted a field into `fields`
    let mut last_end = 0;

    for (i, region) in regions.regions.iter().enumerate() {
        // Check if we need padding
        let pad = region.offset - last_end;
        if pad != 0 {
            let name = Ident::from(format!("_reserved{}", i));
            let pad = pad as usize;
            fields.append(quote! {
                #name : [u8; #pad],
            });
        }

        let mut region_fields = Tokens::new();
        let is_region_a_union = region.is_union();

        for reg_block_field in &region.fields {
            let comment = &format!(
                "0x{:02x} - {}",
                reg_block_field.offset,
                util::escape_brackets(util::respace(&reg_block_field.description).as_ref()),
            )[..];

            if is_region_a_union {
                let name = &reg_block_field.field.ident;
                let mut_name = Ident::from(format!("{}_mut", name.as_ref().unwrap()));
                let ty = &reg_block_field.field.ty;
                let offset = reg_block_field.offset as usize;
                have_accessors = true;
                accessors.append(quote! {
                    #[doc = #comment]
                    #[inline(always)]
                    pub fn #name(&self) -> &#ty {
                        unsafe {
                            &*(((self as *const Self) as *const u8).add(#offset) as *const #ty)
                        }
                    }

                    #[doc = #comment]
                    #[inline(always)]
                    pub fn #mut_name(&self) -> &mut #ty {
                        unsafe {
                            &mut *(((self as *const Self) as *mut u8).add(#offset) as *mut #ty)
                        }
                    }
                });
            } else {
                region_fields.append(quote! {
                    #[doc = #comment]
                });

                reg_block_field.field.to_tokens(&mut region_fields);
                Ident::from(",").to_tokens(&mut region_fields);
            }
        }

        if !is_region_a_union {
            fields.append(&region_fields);
        } else {
            // Emit padding for the items that we're not emitting
            // as fields so that subsequent fields have the correct
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
            let name = Ident::from(format!("_reserved_{}_{}", i, region.compute_ident().unwrap_or_else(|| format!("{}_{}", region.offset, region.end))));
            let pad = (region.end - region.offset) as usize;
            fields.append(quote! {
                #name: [u8; #pad],
            })
        }
        last_end = region.end;
    }

    let name = Ident::from(match name {
        Some(name) => name.to_sanitized_upper_case(),
        None => "RegisterBlock".into(),
    });

    let accessors = if have_accessors {
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
            #fields
        }

        #accessors
    })
}

/// Expand a list of parsed `Register`s or `Cluster`s, and render them to
/// `RegisterBlockField`s containing `Field`s.
fn expand(
    ercs: &[RegisterCluster],
    defs: &Defaults,
    name: Option<&str>,
) -> Result<Vec<RegisterBlockField>> {
    let mut ercs_expanded = vec![];

    for erc in ercs {
        ercs_expanded.extend(match &erc {
            RegisterCluster::Register(register) => expand_register(register, defs, name)?,
            RegisterCluster::Cluster(cluster) => expand_cluster(cluster, defs)?,
        });
    }

    ercs_expanded.sort_by_key(|x| x.offset);

    Ok(ercs_expanded)
}

/// Recursively calculate the size of a cluster. A cluster's size is the maximum
/// end position of its recursive children.
fn cluster_size_in_bits(info: &ClusterInfo, defs: &Defaults) -> Result<u32> {
    let mut size = 0;

    for c in &info.children {
        let end = match c {
            RegisterCluster::Register(reg) => {
                let reg_size: u32 = expand_register(reg, defs, None)?
                    .iter()
                    .map(|rbf| rbf.size)
                    .sum();

                (reg.address_offset * BITS_PER_BYTE) + reg_size
            }
            RegisterCluster::Cluster(clust) => {
                (clust.address_offset * BITS_PER_BYTE) + cluster_size_in_bits(clust, defs)?
            }
        };

        size = size.max(end);
    }
    Ok(size)
}

/// Render a given cluster (and any children) into `RegisterBlockField`s
fn expand_cluster(cluster: &Cluster, defs: &Defaults) -> Result<Vec<RegisterBlockField>> {
    let mut cluster_expanded = vec![];

    let reg_size = cluster.size.or(defs.size);
    let mut defs = defs.clone();
    defs.size = reg_size;

    let cluster_size = cluster_size_in_bits(cluster, &defs)
        .chain_err(|| format!("Cluster {} has no determinable `size` field", cluster.name))?;

    match cluster {
        Cluster::Single(info) => cluster_expanded.push(RegisterBlockField {
            field: convert_svd_cluster(cluster),
            description: info.description.clone(),
            offset: info.address_offset,
            size: cluster_size,
        }),
        Cluster::Array(info, array_info) => {
            let sequential_addresses = cluster_size == array_info.dim_increment * BITS_PER_BYTE;

            // if dimIndex exists, test if it is a sequence of numbers from 0 to dim
            let sequential_indexes = array_info.dim_index.as_ref().map_or(true, |dim_index| {
                dim_index
                    .iter()
                    .map(|element| element.parse::<u32>())
                    .eq((0..array_info.dim).map(Ok))
            });

            let array_convertible = sequential_indexes && sequential_addresses;

            if array_convertible {
                cluster_expanded.push(RegisterBlockField {
                    field: convert_svd_cluster(&cluster),
                    description: info.description.clone(),
                    offset: info.address_offset,
                    size: cluster_size * array_info.dim,
                });
            } else {
                for (field_num, field) in expand_svd_cluster(cluster).iter().enumerate() {
                    cluster_expanded.push(RegisterBlockField {
                        field: field.clone(),
                        description: info.description.clone(),
                        offset: info.address_offset + field_num as u32 * array_info.dim_increment,
                        size: cluster_size,
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
    defs: &Defaults,
    name: Option<&str>,
) -> Result<Vec<RegisterBlockField>> {
    let mut register_expanded = vec![];

    let register_size = register
        .size
        .or(defs.size)
        .ok_or_else(|| format!("Register {} has no `size` field", register.name))?;

    match register {
        Register::Single(info) => register_expanded.push(RegisterBlockField {
            field: convert_svd_register(register, name),
            description: info.description.clone().unwrap(),
            offset: info.address_offset,
            size: register_size,
        }),
        Register::Array(info, array_info) => {
            let sequential_addresses = register_size == array_info.dim_increment * BITS_PER_BYTE;

            // if dimIndex exists, test if it is a sequence of numbers from 0 to dim
            let sequential_indexes = array_info.dim_index.as_ref().map_or(true, |dim_index| {
                dim_index
                    .iter()
                    .map(|element| element.parse::<u32>())
                    .eq((0..array_info.dim).map(Ok))
            });

            let array_convertible = sequential_indexes && sequential_addresses;

            if array_convertible {
                register_expanded.push(RegisterBlockField {
                    field: convert_svd_register(&register, name),
                    description: info.description.clone().unwrap(),
                    offset: info.address_offset,
                    size: register_size * array_info.dim,
                });
            } else {
                for (field_num, field) in expand_svd_register(register, name).iter().enumerate() {
                    register_expanded.push(RegisterBlockField {
                        field: field.clone(),
                        description: info.description.clone().unwrap(),
                        offset: info.address_offset + field_num as u32 * array_info.dim_increment,
                        size: register_size,
                    });
                }
            }
        }
    }

    Ok(register_expanded)
}

/// Render a Cluster Block into `Tokens`
fn cluster_block(
    c: &Cluster,
    defaults: &Defaults,
    p: &Peripheral,
    all_peripherals: &[Peripheral],
    nightly: bool,
) -> Result<Tokens> {
    let mut mod_items: Vec<Tokens> = vec![];

    // name_sc needs to take into account array type.
    let description = util::escape_brackets(util::respace(&c.description).as_ref());

    // Generate the register block.
    let mod_name = match c {
        Cluster::Single(info) => &info.name,
        Cluster::Array(info, _ai) => &info.name,
    }
    .replace("[%s]", "")
    .replace("%s", "");
    let name_sc = Ident::from(&*mod_name.to_sanitized_snake_case());

    let reg_size = c.size.or(defaults.size);
    let mut defaults = defaults.clone();
    defaults.size = reg_size;

    let reg_block = register_or_cluster_block(&c.children, &defaults, Some(&mod_name), nightly)?;

    // Generate definition for each of the registers.
    let registers = util::only_registers(&c.children);
    for reg in &registers {
        mod_items.extend(register::render(
            reg,
            &registers,
            p,
            all_peripherals,
            &defaults,
        )?);
    }

    // Generate the sub-cluster blocks.
    let clusters = util::only_clusters(&c.children);
    for c in &clusters {
        mod_items.push(cluster_block(c, &defaults, p, all_peripherals, nightly)?);
    }

    Ok(quote! {
        #reg_block

        ///Register block
        #[doc = #description]
        pub mod #name_sc {
            #(#mod_items)*
        }
    })
}

/// Takes a svd::Register which may be a register array, and turn in into
/// a list of syn::Field where the register arrays have been expanded.
fn expand_svd_register(register: &Register, name: Option<&str>) -> Vec<syn::Field> {
    let name_to_ty = |name: &String, ns: Option<&str>| -> syn::Ty {
        let ident = if let Some(ns) = ns {
            Cow::Owned(
                String::from("self::")
                    + &ns.to_sanitized_snake_case()
                    + "::"
                    + &name.to_sanitized_upper_case(),
            )
        } else {
            name.to_sanitized_upper_case()
        };

        syn::Ty::Path(
            None,
            syn::Path {
                global: false,
                segments: vec![syn::PathSegment {
                    ident: Ident::from(ident),
                    parameters: syn::PathParameters::none(),
                }],
            },
        )
    };

    let mut out = vec![];

    match register {
        Register::Single(_info) => out.push(convert_svd_register(register, name)),
        Register::Array(info, array_info) => {
            let has_brackets = info.name.contains("[%s]");

            let indices = array_info
                .dim_index
                .as_ref()
                .map(|v| Cow::from(&**v))
                .unwrap_or_else(|| {
                    Cow::from(
                        (0..array_info.dim)
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>(),
                    )
                });

            for (idx, _i) in indices.iter().zip(0..) {
                let nb_name = if has_brackets {
                    info.name.replace("[%s]", idx)
                } else {
                    info.name.replace("%s", idx)
                };

                let ty_name = if has_brackets {
                    info.name.replace("[%s]", "")
                } else {
                    info.name.replace("%s", "")
                };

                let ident = Ident::from(nb_name.to_sanitized_snake_case());
                let ty = name_to_ty(&ty_name, name);

                out.push(syn::Field {
                    ident: Some(ident),
                    vis: syn::Visibility::Public,
                    attrs: vec![],
                    ty,
                });
            }
        }
    }
    out
}

/// Convert a parsed `Register` into its `Field` equivalent
fn convert_svd_register(register: &Register, name: Option<&str>) -> syn::Field {
    let name_to_ty = |name: &String, ns: Option<&str>| -> syn::Ty {
        let ident = if let Some(ns) = ns {
            Cow::Owned(
                String::from("self::")
                    + &ns.to_sanitized_snake_case()
                    + "::"
                    + &name.to_sanitized_upper_case(),
            )
        } else {
            name.to_sanitized_upper_case()
        };

        syn::Ty::Path(
            None,
            syn::Path {
                global: false,
                segments: vec![syn::PathSegment {
                    ident: Ident::from(ident),
                    parameters: syn::PathParameters::none(),
                }],
            },
        )
    };

    match register {
        Register::Single(info) => syn::Field {
            ident: Some(Ident::from(info.name.to_sanitized_snake_case())),
            vis: syn::Visibility::Public,
            attrs: vec![],
            ty: name_to_ty(&info.name, name),
        },
        Register::Array(info, array_info) => {
            let has_brackets = info.name.contains("[%s]");

            let nb_name = if has_brackets {
                info.name.replace("[%s]", "")
            } else {
                info.name.replace("%s", "")
            };

            let ident = Ident::from(nb_name.to_sanitized_snake_case());

            let ty = syn::Ty::Array(
                Box::new(name_to_ty(&nb_name, name)),
                syn::ConstExpr::Lit(syn::Lit::Int(
                    u64::from(array_info.dim),
                    syn::IntTy::Unsuffixed,
                )),
            );

            syn::Field {
                ident: Some(ident),
                vis: syn::Visibility::Public,
                attrs: vec![],
                ty,
            }
        }
    }
}

/// Takes a svd::Cluster which may contain a register array, and turn in into
/// a list of syn::Field where the register arrays have been expanded.
fn expand_svd_cluster(cluster: &Cluster) -> Vec<syn::Field> {
    let name_to_ty = |name: &String| -> syn::Ty {
        syn::Ty::Path(
            None,
            syn::Path {
                global: false,
                segments: vec![syn::PathSegment {
                    ident: Ident::from(name.to_sanitized_upper_case()),
                    parameters: syn::PathParameters::none(),
                }],
            },
        )
    };

    let mut out = vec![];

    match &cluster {
        Cluster::Single(_info) => out.push(convert_svd_cluster(cluster)),
        Cluster::Array(info, array_info) => {
            let has_brackets = info.name.contains("[%s]");

            let indices = array_info
                .dim_index
                .as_ref()
                .map(|v| Cow::from(&**v))
                .unwrap_or_else(|| {
                    Cow::from(
                        (0..array_info.dim)
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>(),
                    )
                });

            for (idx, _i) in indices.iter().zip(0..) {
                let name = if has_brackets {
                    info.name.replace("[%s]", idx)
                } else {
                    info.name.replace("%s", idx)
                };

                let ty_name = if has_brackets {
                    info.name.replace("[%s]", "")
                } else {
                    info.name.replace("%s", "")
                };

                let ident = Ident::from(name.to_sanitized_snake_case());
                let ty = name_to_ty(&ty_name);

                out.push(syn::Field {
                    ident: Some(ident),
                    vis: syn::Visibility::Public,
                    attrs: vec![],
                    ty,
                });
            }
        }
    }
    out
}

/// Convert a parsed `Cluster` into its `Field` equivalent
fn convert_svd_cluster(cluster: &Cluster) -> syn::Field {
    let name_to_ty = |name: &String| -> syn::Ty {
        syn::Ty::Path(
            None,
            syn::Path {
                global: false,
                segments: vec![syn::PathSegment {
                    ident: Ident::from(name.to_sanitized_upper_case()),
                    parameters: syn::PathParameters::none(),
                }],
            },
        )
    };

    match cluster {
        Cluster::Single(info) => syn::Field {
            ident: Some(Ident::from(info.name.to_sanitized_snake_case())),
            vis: syn::Visibility::Public,
            attrs: vec![],
            ty: name_to_ty(&info.name),
        },
        Cluster::Array(info, array_info) => {
            let has_brackets = info.name.contains("[%s]");

            let name = if has_brackets {
                info.name.replace("[%s]", "")
            } else {
                info.name.replace("%s", "")
            };

            let ident = Ident::from(name.to_sanitized_snake_case());

            let ty = syn::Ty::Array(
                Box::new(name_to_ty(&name)),
                syn::ConstExpr::Lit(syn::Lit::Int(
                    u64::from(array_info.dim),
                    syn::IntTy::Unsuffixed,
                )),
            );

            syn::Field {
                ident: Some(ident),
                vis: syn::Visibility::Public,
                attrs: vec![],
                ty,
            }
        }
    }
}
