use crate::{svd::Peripheral, util, Config, Settings};
use anyhow::Result;
use log::debug;
use proc_macro2::TokenStream;
use quote::quote;
use std::{collections::HashMap, fmt::Write, str::FromStr};

pub fn is_riscv_peripheral(p: &Peripheral, s: &Settings) -> bool {
    // TODO cleaner implementation of this
    match &s.riscv_config {
        Some(c) => {
            c.clint.as_ref().is_some_and(|clint| clint.name == p.name)
                || c.plic.as_ref().is_some_and(|plic| plic.name == p.name)
        }
        _ => false,
    }
}

/// Whole RISC-V generation
pub fn render(
    peripherals: &[Peripheral],
    device_x: &mut String,
    config: &Config,
) -> Result<TokenStream> {
    let mut mod_items = TokenStream::new();

    let defmt = config
        .impl_defmt
        .as_ref()
        .map(|feature| quote!(#[cfg_attr(feature = #feature, derive(defmt::Format))]));

    if let Some(c) = config.settings.riscv_config.as_ref() {
        if !c.core_interrupts.is_empty() {
            debug!("Rendering target-specific core interrupts");
            writeln!(device_x, "/* Core interrupt sources and trap handlers */")?;
            let mut interrupts = vec![];
            for interrupt in c.core_interrupts.iter() {
                let name = TokenStream::from_str(&interrupt.name).unwrap();
                let value = TokenStream::from_str(&format!("{}", interrupt.value)).unwrap();
                let description = interrupt.description();

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
                #[riscv::pac_enum(unsafe CoreInterruptNumber)]
                #defmt
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum CoreInterrupt {
                    #(#interrupts)*
                }
            });
        } else {
            // when no interrupts are defined, we re-export the standard riscv interrupts
            mod_items.extend(quote! {pub use riscv::interrupt::Interrupt as CoreInterrupt;});
        }

        if !c.exceptions.is_empty() {
            debug!("Rendering target-specific exceptions");
            writeln!(device_x, "/* Exception sources */")?;
            let mut exceptions = vec![];
            for exception in c.exceptions.iter() {
                let name = TokenStream::from_str(&exception.name).unwrap();
                let value = TokenStream::from_str(&format!("{}", exception.value)).unwrap();
                let description = exception.description();

                writeln!(device_x, "PROVIDE({name} = ExceptionHandler);")?;

                exceptions.push(quote! {
                    #[doc = #description]
                    #name = #value,
                });
            }
            mod_items.extend(quote! {
                /// Exception sources in the device.
                #[riscv::pac_enum(unsafe ExceptionNumber)]
                #defmt
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum Exception {
                    #(#exceptions)*
                }
            });
        } else {
            // when no exceptions are defined, we re-export the standard riscv exceptions
            mod_items.extend(quote! { pub use riscv::interrupt::Exception; });
        }

        if !c.priorities.is_empty() {
            debug!("Rendering target-specific priority levels");
            let priorities = c.priorities.iter().map(|priority| {
                let name = TokenStream::from_str(&priority.name).unwrap();
                let value = TokenStream::from_str(&format!("{}", priority.value)).unwrap();
                let description = priority.description();

                quote! {
                    #[doc = #description]
                    #name = #value,
                }
            });
            mod_items.extend(quote! {
                /// Priority levels in the device
                #[riscv::pac_enum(unsafe PriorityNumber)]
                #defmt
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum Priority {
                    #(#priorities)*
                }
            });
        }

        if !c.harts.is_empty() {
            debug!("Rendering target-specific HART IDs");
            let harts = c.harts.iter().map(|hart| {
                let name = TokenStream::from_str(&hart.name).unwrap();
                let value = TokenStream::from_str(&format!("{}", hart.value)).unwrap();
                let description = hart.description();

                quote! {
                    #[doc = #description]
                    #name = #value,
                }
            });
            mod_items.extend(quote! {
                /// HARTs in the device
                #[riscv::pac_enum(unsafe HartIdNumber)]
                #defmt
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum Hart {
                    #(#harts)*
                }
            });
        }
    } else {
        // when no riscv block is defined, we re-export the standard riscv interrupt and exception enums
        mod_items.extend(quote! {
            pub use riscv::interrupt::{Interrupt as CoreInterrupt, Exception};
        });
    }

    mod_items.extend(quote! {
        pub use riscv::{
            InterruptNumber, ExceptionNumber, PriorityNumber, HartIdNumber,
            interrupt::{enable, disable, free, nested}
        };

        pub type Trap = riscv::interrupt::Trap<CoreInterrupt, Exception>;

        /// Retrieves the cause of a trap in the current hart.
        ///
        /// If the raw cause is not a valid interrupt or exception for the target, it returns an error.
        #[inline]
        pub fn try_cause() -> riscv::result::Result<Trap> {
            riscv::interrupt::try_cause()
        }

        /// Retrieves the cause of a trap in the current hart (machine mode).
        ///
        /// If the raw cause is not a valid interrupt or exception for the target, it panics.
        #[inline]
        pub fn cause() -> Trap {
            try_cause().unwrap()
        }
    });

    let external_interrupts = peripherals
        .iter()
        .flat_map(|p| p.interrupt.iter())
        .map(|i| (i.value, i))
        .collect::<HashMap<_, _>>();
    let mut external_interrupts = external_interrupts.into_values().collect::<Vec<_>>();
    external_interrupts.sort_by_key(|i| i.value);
    if !external_interrupts.is_empty() {
        debug!("Rendering target-specific external interrupts");
        writeln!(device_x, "/* External interrupt sources */")?;
        let mut interrupts = vec![];
        for i in external_interrupts.iter() {
            let name = TokenStream::from_str(&i.name).unwrap();
            let value = TokenStream::from_str(&format!("{}", i.value)).unwrap();
            let description = format!(
                "{} - {}",
                i.value,
                i.description
                    .as_ref()
                    .map(|s| util::escape_special_chars(s))
                    .as_ref()
                    .map(|s| util::respace(s))
                    .unwrap_or_else(|| i.name.as_str().into())
            );

            writeln!(device_x, "PROVIDE({name} = DefaultHandler);")?;

            interrupts.push(quote! {
                #[doc = #description]
                #name = #value,
            })
        }
        mod_items.extend(quote! {
            /// External interrupts. These interrupts are handled by the external peripherals.
            #[riscv::pac_enum(unsafe ExternalInterruptNumber)]
            #defmt
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum ExternalInterrupt {
                #(#interrupts)*
            }
        });
    }

    let mut riscv_peripherals = TokenStream::new();
    if let Some(c) = config.settings.riscv_config.as_ref() {
        let harts = match c.harts.is_empty() {
            true => vec![],
            false => c
                .harts
                .iter()
                .map(|h| (TokenStream::from_str(&h.name).unwrap(), h.value))
                .collect::<Vec<_>>(),
        };
        if let Some(clint) = &c.clint {
            let p = peripherals.iter().find(|&p| p.name == clint.name).unwrap();
            let base = TokenStream::from_str(&format!("base 0x{:X},", p.base_address)).unwrap();
            let freq = match clint.freq {
                Some(clk) => match clint.async_delay {
                    true => TokenStream::from_str(&format!("freq {clk}, async_delay,")).unwrap(),
                    false => TokenStream::from_str(&format!("freq {clk},")).unwrap(),
                },
                None => quote! {},
            };
            let mtimecmps = harts
                .iter()
                .map(|(name, value)| {
                    let mtimecmp_name = TokenStream::from_str(&format!("mtimecmp{value}")).unwrap();
                    let doc = format!("[{value}](crate::interrupt::Hart::{name})");
                    quote! {#mtimecmp_name = (crate::interrupt::Hart::#name, #doc)}
                })
                .collect::<Vec<_>>();
            let mtimecmps = match mtimecmps.len() {
                0 => quote! {},
                _ => quote! {mtimecmps [ #(#mtimecmps),* ],},
            };
            let msips = harts
                .iter()
                .map(|(name, value)| {
                    let msip_name = TokenStream::from_str(&format!("msip{value}")).unwrap();
                    let doc = format!("[{value}](crate::interrupt::Hart::{name})");
                    quote! {#msip_name = (crate::interrupt::Hart::#name, #doc)}
                })
                .collect::<Vec<_>>();
            let msips = match msips.len() {
                0 => quote! {},
                _ => quote! {msips [ #(#msips),* ],},
            };

            riscv_peripherals.extend(quote! {
                riscv_peripheral::clint_codegen!(#base #freq #mtimecmps #msips);
            });
        }
        if let Some(plic) = &c.plic {
            let p = peripherals.iter().find(|&p| p.name == plic.name).unwrap();
            let base = TokenStream::from_str(&format!("base 0x{:X},", p.base_address)).unwrap();
            let ctxs = harts
                .iter()
                .map(|(name, value)| {
                    let ctx_name = TokenStream::from_str(&format!("ctx{value}")).unwrap();
                    let doc = format!("[{value}](crate::interrupt::Hart::{name})");
                    quote! {#ctx_name = (crate::interrupt::Hart::#name, #doc)}
                })
                .collect::<Vec<_>>();
            let ctxs = match ctxs.len() {
                0 => quote! {},
                _ => quote! {ctxs [ #(#ctxs),* ],},
            };

            riscv_peripherals.extend(quote! {
                riscv_peripheral::plic_codegen!(#base #ctxs);
            });

            if let Some(core_interrupt) = &plic.core_interrupt {
                let core_interrupt = TokenStream::from_str(core_interrupt).unwrap();
                let ctx = match &plic.hart_id {
                    Some(hart_id) => {
                        TokenStream::from_str(&format!("ctx(Hart::{hart_id})")).unwrap()
                    }
                    None => quote! { ctx_mhartid() },
                };
                mod_items.extend(quote! {
                    #[cfg(feature = "rt")]
                    #[riscv_rt::core_interrupt(CoreInterrupt::#core_interrupt)]
                    fn plic_handler() {
                        let claim = crate::PLIC::#ctx.claim();
                        if let Some(s) = claim.claim::<ExternalInterrupt>() {
                            unsafe { _dispatch_external_interrupt(s.number()) }
                            claim.complete(s);
                        }
                    }
                });
            }
        }
    }

    Ok(quote! {
        /// Interrupt numbers, priority levels, and HART IDs.
        pub mod interrupt {
            #mod_items
        }
        #riscv_peripherals
    })
}
