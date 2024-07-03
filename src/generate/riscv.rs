use crate::{
    svd::{Peripheral, Riscv},
    util,
};
use anyhow::Result;
use proc_macro2::TokenStream;
use quote::quote;
use std::{collections::HashMap, fmt::Write, str::FromStr};

/// Whole RISC-V generation
pub fn render(
    r: &Riscv,
    peripherals: &[Peripheral],
    device_x: &mut String, // TODO
) -> Result<TokenStream> {
    let mut mod_items = TokenStream::new();

    let external_interrupts = peripherals
        .iter()
        .flat_map(|p| p.interrupt.iter())
        .map(|i| (i.value, i))
        .collect::<HashMap<_, _>>();
    let mut external_interrupts = external_interrupts.into_values().collect::<Vec<_>>();
    external_interrupts.sort_by_key(|i| i.value);
    if !external_interrupts.is_empty() {
        writeln!(device_x, "/* External interrupt sources */")?;
        mod_items.extend(quote! { pub use riscv_pac::ExternalInterruptNumber; });

        let mut interrupts = vec![];
        for i in external_interrupts.iter() {
            let name = TokenStream::from_str(&i.name).unwrap();
            let value = TokenStream::from_str(&format!("{}", i.value)).unwrap();
            let description = format!(
                "{} - {}",
                i.value,
                i.description
                    .as_ref()
                    .map(|s| util::respace(s))
                    .as_ref()
                    .map(|s| util::escape_special_chars(s))
                    .unwrap_or_else(|| i.name.clone())
            );

            writeln!(device_x, "PROVIDE({name} = DefaultHandler);")?;

            interrupts.push(quote! {
                #[doc = #description]
                #name = #value,
            })
        }
        mod_items.extend(quote! {
            /// External interrupts. These interrupts are handled by the external peripherals.
            #[repr(usize)]
            #[riscv_pac::pac_enum(unsafe ExternalInterruptNumber)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum ExternalInterrupt {
                #(#interrupts)*
            }
        });
    }

    if !r.core_interrupts.is_empty() {
        writeln!(device_x, "/* Core interrupt sources and trap handlers */")?;
        mod_items.extend(quote! { pub use riscv_pac::CoreInterruptNumber; });

        let mut interrupts = vec![];
        for i in r.core_interrupts.iter() {
            let name = TokenStream::from_str(&i.name).unwrap();
            let value = TokenStream::from_str(&format!("{}", i.value)).unwrap();
            let description = format!(
                "{} - {}",
                i.value,
                i.description
                    .as_ref()
                    .map(|s| util::respace(s))
                    .as_ref()
                    .map(|s| util::escape_special_chars(s))
                    .unwrap_or_else(|| i.name.clone())
            );

            writeln!(device_x, "PROVIDE({name} = DefaultHandler);")?;
            writeln!(
                device_x,
                "PROVIDE(_start_{name}_trap = _start_DefaultHandler_trap);"
            )?;

            interrupts.push(quote! {
                #[doc = #description]
                #name = #value,
            });
        }
        mod_items.extend(quote! {
            /// Core interrupts. These interrupts are handled by the core itself.
            #[repr(usize)]
            #[riscv_pac::pac_enum(unsafe CoreInterruptNumber)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum CoreInterrupt {
                #(#interrupts)*
            }
        });
    }

    if !r.priorities.is_empty() {
        mod_items.extend(quote! { pub use riscv_pac::PriorityNumber; });
        let priorities = r.priorities.iter().map(|p| {
            let name = TokenStream::from_str(&p.name).unwrap();
            let value = TokenStream::from_str(&format!("{}", p.value)).unwrap();
            let description = format!(
                "{} - {}",
                p.value,
                p.description
                    .as_ref()
                    .map(|s| util::respace(s))
                    .as_ref()
                    .map(|s| util::escape_special_chars(s))
                    .unwrap_or_else(|| p.name.clone())
            );

            quote! {
                #[doc = #description]
                #name = #value,
            }
        });
        mod_items.extend(quote! {
            /// Priority levels in the device
            #[repr(u8)]
            #[riscv_pac::pac_enum(unsafe PriorityNumber)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum Priority {
                #(#priorities)*
            }
        });
    }

    if !r.harts.is_empty() {
        mod_items.extend(quote! { pub use riscv_pac::HartIdNumber; });
        let harts = r.harts.iter().map(|h| {
            let name = TokenStream::from_str(&h.name).unwrap();
            let value = TokenStream::from_str(&format!("{}", h.value)).unwrap();
            let description = format!(
                "{} - {}",
                h.value,
                h.description
                    .as_ref()
                    .map(|s| util::respace(s))
                    .as_ref()
                    .map(|s| util::escape_special_chars(s))
                    .unwrap_or_else(|| h.name.clone())
            );

            quote! {
                #[doc = #description]
                #name = #value,
            }
        });
        mod_items.extend(quote! {
            /// HARTs in the device
            #[repr(u16)]
            #[riscv_pac::pac_enum(unsafe HartIdNumber)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum Hart {
                #(#harts)*
            }
        });
    }

    Ok(quote! {
        /// Interrupt numbers, priority levels, and HART IDs.
        pub mod interrupt {
            #mod_items
        }
    })
}
