use std::io::{self, Write};
use std::borrow::Cow;

use quote::{ToTokens, Tokens};
use svd::{Defaults, Peripheral, Register};
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

    if derived {
        return Ok(out);
    }

    let registers = p.registers.as_ref().map(|x| x.as_ref()).unwrap_or(&[][..]);

    // No `struct RegisterBlock` can be generated
    if registers.is_empty() {
        // Drop the `#name_pc` definition of the peripheral
        out.pop();
        return Ok(out);
    }

    let mut mod_items = vec![];
    mod_items.push(register_block(registers, defaults)?);

    for reg in registers {
        out.extend(register::render(
            reg,
            registers,
            p,
            all_peripherals,
            defaults
        )?);
    }

    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));
    out.push(quote! {
        #[doc = #description]
        pub mod #name_sc {
            use vcell::VolatileCell;

            #(#mod_items)*
        }
    });

    Ok(out)
}

struct RegisterBlockField {
    field: syn::Field,
    description: String,
    offset: u32,
    size: u32,
}

fn register_block(registers: &[Register], defs: &Defaults) -> Result<Tokens> {
    let mut fields = Tokens::new();
    // enumeration of reserved fields
    let mut i = 0;
    // offset from the base address, in bytes
    let mut offset = 0;
    let mut registers_expanded = vec![];

    // If svd register arrays can't be converted to rust arrays (non sequential adresses, non
    // numeral indexes, or not containing all elements from 0 to size) they will be expanded
    for register in registers {
        let register_size = register
            .size
            .or(defs.size)
            .ok_or_else(|| format!("Register {} has no `size` field", register.name))?;

        match *register {
            Register::Single(ref info) => registers_expanded.push(RegisterBlockField {
                field: convert_svd_register(register),
                description: info.description.clone(),
                offset: info.address_offset,
                size: register_size,
            }),
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
                    registers_expanded.push(RegisterBlockField {
                        field: convert_svd_register(&register),
                        description: info.description.clone(),
                        offset: info.address_offset,
                        size: register_size * array_info.dim,
                    });
                } else {
                    let mut field_num = 0;
                    for field in expand_svd_register(register).iter() {
                        registers_expanded.push(RegisterBlockField {
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
    }

    registers_expanded.sort_by_key(|x| x.offset);

    for register in registers_expanded {
        let pad = if let Some(pad) = register.offset.checked_sub(offset) {
            pad
        } else {
            writeln!(
                io::stderr(),
                "WARNING {} overlaps with another register at offset {}. \
                 Ignoring.",
                register.field.ident.unwrap(),
                register.offset
            ).ok();
            continue;
        };

        if pad != 0 {
            let name = Ident::new(format!("_reserved{}", i));
            let pad = pad as usize;
            fields.append(quote! {
                #name : [u8; #pad],
            });
            i += 1;
        }

        let comment = &format!(
            "0x{:02x} - {}",
            register.offset,
            util::respace(&register.description),
        )[..];

        fields.append(quote! {
            #[doc = #comment]
        });

        register.field.to_tokens(&mut fields);
        Ident::new(",").to_tokens(&mut fields);

        offset = register.offset + register.size / BITS_PER_BYTE;
    }

    Ok(quote! {
        /// Register block
        #[repr(C)]
        pub struct RegisterBlock {
            #fields
        }
    })
}


/// Takes a svd::Register which may be a register array, and turn in into
/// a list of syn::Field where the register arrays have been expanded.
fn expand_svd_register(register: &Register) -> Vec<syn::Field> {
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

    match *register {
        Register::Single(ref _info) => out.push(convert_svd_register(register)),
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

fn convert_svd_register(register: &Register) -> syn::Field {
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

    match *register {
        Register::Single(ref info) => syn::Field {
            ident: Some(Ident::new(info.name.to_sanitized_snake_case())),
            vis: syn::Visibility::Public,
            attrs: vec![],
            ty: name_to_ty(&info.name),
        },
        Register::Array(ref info, ref array_info) => {
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
