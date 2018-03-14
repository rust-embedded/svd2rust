use std::borrow::Cow;

use either::Either;
use quote::{ToTokens, Tokens};
use svd::{Cluster, ClusterInfo, Defaults, Peripheral, Register};
use syn::{self, Ident};

use errors::*;
use util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase, BITS_PER_BYTE};

use generate::register;

pub fn render(
    p: &Peripheral,
    all_peripherals: &[Peripheral],
    defaults: &Defaults,
) -> Result<Vec<Tokens>> {
    let mut out = vec![];

    let name_pc = Ident::new(&*p.name.to_sanitized_upper_case());
    let address = util::hex(p.base_address);
    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));

    let name_sc = Ident::new(&*p.name.to_sanitized_snake_case());
    let (base, derived) = if let Some(base) = p.derived_from.as_ref() {
        // TODO Verify that base exists
        // TODO We don't handle inheritance style `derivedFrom`, we should raise
        // an error in that case
        (Ident::new(&*base.to_sanitized_snake_case()), true)
    } else {
        (name_sc.clone(), false)
    };

    // Insert the peripheral structure
    out.push(quote! {
        #[doc = #description]
        pub struct #name_pc { _marker: PhantomData<*const ()> }

        unsafe impl Send for #name_pc {}

        impl #name_pc {
            /// Returns a pointer to the register block
            pub fn ptr() -> *const #base::RegisterBlock {
                #address as *const _
            }
        }

        impl Deref for #name_pc {
            type Target = #base::RegisterBlock;

            fn deref(&self) -> &#base::RegisterBlock {
                unsafe { &*#name_pc::ptr() }
            }
        }
    });

    // Derived peripherals do not require re-implementation, and will instead
    // use a single definition of the non-derived version
    if derived {
        return Ok(out);
    }

    // erc: *E*ither *R*egister or *C*luster
    let ercs = p.registers.as_ref().map(|x| x.as_ref()).unwrap_or(&[][..]);

    let registers: &[&Register] = &util::only_registers(&ercs)[..];
    let clusters = util::only_clusters(ercs);

    // No `struct RegisterBlock` can be generated
    if registers.is_empty() && clusters.is_empty() {
        // Drop the definition of the peripheral
        out.pop();
        return Ok(out);
    }

    // Push any register or cluster blocks into the output
    let mut mod_items = vec![];
    mod_items.push(register_or_cluster_block(ercs, defaults, None)?);

    // Push all cluster related information into the peripheral module
    for c in &clusters {
        mod_items.push(cluster_block(c, defaults, p, all_peripherals)?);
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

    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));
    out.push(quote! {
        #[doc = #description]
        pub mod #name_sc {
            #(#mod_items)*
        }
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
    fn add(&mut self, field: &RegisterBlockField) {

        // When merging, this holds the indices in self.regions
        // that the input `field` will be merging with.
        let mut indices = Vec::new();

        let field_start = field.offset;
        let field_end = field_start + field.size / BITS_PER_BYTE;

        // The region that we're going to insert
        let mut new_region = Region {
            fields: vec![field.clone()],
            offset: field.offset,
            end: field.offset + field.size / BITS_PER_BYTE
        };

        // Locate existing region(s) that we intersect with and
        // fold them into the new region we're creating.  There
        // may be multiple regions that we intersect with, so
        // we keep looping to find them all.
        for (idx, mut f) in self.regions.iter_mut().enumerate() {
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
        let idx = self.regions.binary_search_by_key(&new_region.offset, |r| r.offset);
        match idx {
            Ok(idx) => {
                panic!("we shouldn't exist in the vec, but are at idx {} {:#?}\n{:#?}",
                    idx, new_region, self.regions);
            }
            Err(idx) => self.regions.insert(idx, new_region)
        }
    }
}

fn register_or_cluster_block(
    ercs: &[Either<Register, Cluster>],
    defs: &Defaults,
    name: Option<&str>,
) -> Result<Tokens> {
    let mut fields = Tokens::new();

    let ercs_expanded = expand(ercs, defs, name)?;

    // Locate conflicting regions; we'll need to use unions to represent them.
    let mut regions = FieldRegions::default();

    for reg_block_field in &ercs_expanded {
        regions.add(reg_block_field);
    }

    // The end of the region from the prior iteration of the loop
    let mut last_end = None;

    for (i, region) in regions.regions.iter().enumerate() {
        // Check if we need padding
        if let Some(end) = last_end {
            let pad = region.offset - end;
            if pad != 0 {
                let name = Ident::new(format!("_reserved{}", i));
                let pad = pad as usize;
                fields.append(quote! {
                    #name : [u8; #pad],
                });
            }
        }

        last_end = Some(region.end);

        if region.fields.len() > 1 {
            // TODO: this is where we'd emit a union container
            eprintln!("WARNING: overlaps for region offset={}-{}. \
                Using the first one of these:",
                region.offset, region.end-1);

            for f in &region.fields {
                eprintln!("  {:?} {}-{}", f.field.ident, f.offset,
                    (f.offset + f.size / BITS_PER_BYTE)-1);
            }
        }

        let reg_block_field = &region.fields[0];

        let comment = &format!(
            "0x{:02x} - {}",
            reg_block_field.offset,
            util::respace(&reg_block_field.description),
        )[..];

        fields.append(quote! {
            #[doc = #comment]
        });

        reg_block_field.field.to_tokens(&mut fields);
        Ident::new(",").to_tokens(&mut fields);
    }

    let name = Ident::new(match name {
        Some(name) => name.to_sanitized_upper_case(),
        None => "RegisterBlock".into(),
    });

    Ok(quote! {
        /// Register block
        #[repr(C)]
        pub struct #name {
            #fields
        }
    })
}

/// Expand a list of parsed `Register`s or `Cluster`s, and render them to
/// `RegisterBlockField`s containing `Field`s.
fn expand(
    ercs: &[Either<Register, Cluster>],
    defs: &Defaults,
    name: Option<&str>,
) -> Result<Vec<RegisterBlockField>> {
    let mut ercs_expanded = vec![];

    for erc in ercs {
        ercs_expanded.extend(match erc {
            &Either::Left(ref register) => expand_register(register, defs, name)?,
            &Either::Right(ref cluster) => expand_cluster(cluster, defs)?,
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
        let end = match *c {
            Either::Left(ref reg) => {
                let reg_size: u32 = expand_register(reg, defs, None)?
                    .iter()
                    .map(|rbf| rbf.size)
                    .sum();

                (reg.address_offset * BITS_PER_BYTE) + reg_size
            }
            Either::Right(ref clust) => {
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


    let cluster_size = cluster
        .size
        .ok_or_else(|| format!("Cluster {} has no explictly defined size", cluster.name))
        .or_else(|_e| cluster_size_in_bits(cluster, defs))
        .chain_err(|| format!("Cluster {} has no determinable `size` field", cluster.name))?;

    match *cluster {
        Cluster::Single(ref info) => {
            cluster_expanded.push(RegisterBlockField {
                field: convert_svd_cluster(cluster),
                description: info.description.clone(),
                offset: info.address_offset,
                size: cluster_size,
            })
        },
        Cluster::Array(ref info, ref array_info) => {
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
                let mut field_num = 0;
                for field in expand_svd_cluster(cluster).iter() {
                    cluster_expanded.push(RegisterBlockField {
                        field: field.clone(),
                        description: info.description.clone(),
                        offset: info.address_offset + field_num * array_info.dim_increment,
                        size: cluster_size,
                    });
                    field_num += 1;
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

    match *register {
        Register::Single(ref info) => {
            register_expanded.push(RegisterBlockField {
                field: convert_svd_register(register, name),
                description: info.description.clone(),
                offset: info.address_offset,
                size: register_size,
            })
        },
        Register::Array(ref info, ref array_info) => {
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
                    description: info.description.clone(),
                    offset: info.address_offset,
                    size: register_size * array_info.dim,
                });
            } else {
                let mut field_num = 0;
                for field in expand_svd_register(register, name).iter() {
                    register_expanded.push(RegisterBlockField {
                        field: field.clone(),
                        description: info.description.clone(),
                        offset: info.address_offset + field_num * array_info.dim_increment,
                        size: register_size,
                    });
                    field_num += 1;
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
) -> Result<Tokens> {
    let mut mod_items: Vec<Tokens> = vec![];

    // name_sc needs to take into account array type.
    let description = util::respace(&c.description);

    // Generate the register block.
    let mod_name = match *c {
        Cluster::Single(ref info) => &info.name,
        Cluster::Array(ref info, ref _ai) => &info.name,
    }.replace("[%s]", "")
        .replace("%s", "");
    let name_sc = Ident::new(&*mod_name.to_sanitized_snake_case());
    let reg_block = register_or_cluster_block(&c.children, defaults, Some(&mod_name))?;

    // Generate definition for each of the registers.
    let registers = util::only_registers(&c.children);
    for reg in &registers {
        mod_items.extend(register::render(
            reg,
            &registers,
            p,
            all_peripherals,
            defaults,
        )?);
    }

    // Generate the sub-cluster blocks.
    let clusters = util::only_clusters(&c.children);
    for c in &clusters {
        mod_items.push(cluster_block(c, defaults, p, all_peripherals)?);
    }

    Ok(quote! {
        #reg_block

        /// Register block
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
                String::from("self::") + &ns.to_sanitized_snake_case() + "::"
                    + &name.to_sanitized_upper_case(),
            )
        } else {
            name.to_sanitized_upper_case()
        };

        syn::Ty::Path(
            None,
            syn::Path {
                global: false,
                segments: vec![
                    syn::PathSegment {
                        ident: Ident::new(ident),
                        parameters: syn::PathParameters::none(),
                    },
                ],
            },
        )
    };

    let mut out = vec![];

    match *register {
        Register::Single(ref _info) => out.push(convert_svd_register(register, name)),
        Register::Array(ref info, ref array_info) => {
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
                    info.name.replace("[%s]", format!("{}", idx).as_str())
                } else {
                    info.name.replace("%s", format!("{}", idx).as_str())
                };

                let ty_name = if has_brackets {
                    info.name.replace("[%s]", "")
                } else {
                    info.name.replace("%s", "")
                };

                let ident = Ident::new(nb_name.to_sanitized_snake_case());
                let ty = name_to_ty(&ty_name, name);

                out.push(syn::Field {
                    ident: Some(ident),
                    vis: syn::Visibility::Public,
                    attrs: vec![],
                    ty: ty,
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
                String::from("self::") + &ns.to_sanitized_snake_case() + "::"
                    + &name.to_sanitized_upper_case(),
            )
        } else {
            name.to_sanitized_upper_case()
        };

        syn::Ty::Path(
            None,
            syn::Path {
                global: false,
                segments: vec![
                    syn::PathSegment {
                        ident: Ident::new(ident),
                        parameters: syn::PathParameters::none(),
                    },
                ],
            },
        )
    };

    match *register {
        Register::Single(ref info) => syn::Field {
            ident: Some(Ident::new(info.name.to_sanitized_snake_case())),
            vis: syn::Visibility::Public,
            attrs: vec![],
            ty: name_to_ty(&info.name, name),
        },
        Register::Array(ref info, ref array_info) => {
            let has_brackets = info.name.contains("[%s]");

            let nb_name = if has_brackets {
                info.name.replace("[%s]", "")
            } else {
                info.name.replace("%s", "")
            };

            let ident = Ident::new(nb_name.to_sanitized_snake_case());

            let ty = syn::Ty::Array(
                Box::new(name_to_ty(&nb_name, name)),
                syn::ConstExpr::Lit(syn::Lit::Int(array_info.dim as u64, syn::IntTy::Unsuffixed)),
            );

            syn::Field {
                ident: Some(ident),
                vis: syn::Visibility::Public,
                attrs: vec![],
                ty: ty,
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
                segments: vec![
                    syn::PathSegment {
                        ident: Ident::new(name.to_sanitized_upper_case()),
                        parameters: syn::PathParameters::none(),
                    },
                ],
            },
        )
    };

    let mut out = vec![];

    match *cluster {
        Cluster::Single(ref _info) => out.push(convert_svd_cluster(cluster)),
        Cluster::Array(ref info, ref array_info) => {
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
                    info.name.replace("[%s]", format!("{}", idx).as_str())
                } else {
                    info.name.replace("%s", format!("{}", idx).as_str())
                };

                let ty_name = if has_brackets {
                    info.name.replace("[%s]", "")
                } else {
                    info.name.replace("%s", "")
                };

                let ident = Ident::new(name.to_sanitized_snake_case());
                let ty = name_to_ty(&ty_name);

                out.push(syn::Field {
                    ident: Some(ident),
                    vis: syn::Visibility::Public,
                    attrs: vec![],
                    ty: ty,
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
                segments: vec![
                    syn::PathSegment {
                        ident: Ident::new(name.to_sanitized_upper_case()),
                        parameters: syn::PathParameters::none(),
                    },
                ],
            },
        )
    };

    match *cluster {
        Cluster::Single(ref info) => syn::Field {
            ident: Some(Ident::new(info.name.to_sanitized_snake_case())),
            vis: syn::Visibility::Public,
            attrs: vec![],
            ty: name_to_ty(&info.name),
        },
        Cluster::Array(ref info, ref array_info) => {
            let has_brackets = info.name.contains("[%s]");

            let name = if has_brackets {
                info.name.replace("[%s]", "")
            } else {
                info.name.replace("%s", "")
            };

            let ident = Ident::new(name.to_sanitized_snake_case());

            let ty = syn::Ty::Array(
                Box::new(name_to_ty(&name)),
                syn::ConstExpr::Lit(syn::Lit::Int(array_info.dim as u64, syn::IntTy::Unsuffixed)),
            );

            syn::Field {
                ident: Some(ident),
                vis: syn::Visibility::Public,
                attrs: vec![],
                ty: ty,
            }
        }
    }
}
