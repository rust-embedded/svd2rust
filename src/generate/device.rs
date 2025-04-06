use crate::svd::{array::names, Device, Peripheral};
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};

use log::{debug, warn};
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::config::{Config, Target};
use crate::util::{self, ident};
use anyhow::{Context, Result};

use crate::generate::{interrupt, peripheral, riscv};

/// Whole device generation
pub fn render(d: &Device, config: &Config, device_x: &mut String) -> Result<TokenStream> {
    let index = svd_parser::expand::Index::create(d);
    let mut out = TokenStream::new();

    let commit_info = {
        let tmp = include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"));

        if tmp.is_empty() {
            " (untracked)"
        } else {
            tmp
        }
    };

    // make_mod option explicitly disables inner attributes.
    if config.target == Target::Msp430 && !config.make_mod {
        out.extend(quote! {
            #![feature(abi_msp430_interrupt)]
        });
    }

    if !config.skip_crate_attributes {
        let doc = format!(
            "Peripheral access API for {0} microcontrollers \
             (generated using svd2rust v{1}{commit_info})\n\n\
             You can find an overview of the generated API [here].\n\n\
             API features to be included in the [next] svd2rust \
             release can be generated by cloning the svd2rust [repository], \
             checking out the above commit, and running `cargo doc --open`.\n\n\
             [here]: https://docs.rs/svd2rust/{1}/svd2rust/#peripheral-api\n\
             [next]: https://github.com/rust-embedded/svd2rust/blob/master/CHANGELOG.md#unreleased\n\
             [repository]: https://github.com/rust-embedded/svd2rust",
            d.name.to_uppercase(),
            env!("CARGO_PKG_VERSION"),
        );

        out.extend(quote! { #![doc = #doc] });
    }

    if !config.make_mod && !config.skip_crate_attributes {
        out.extend(quote! {
            // Explicitly allow a few warnings that may be verbose
            #![allow(non_camel_case_types)]
            #![allow(non_snake_case)]
            #![no_std]
            #![cfg_attr(docsrs, feature(doc_auto_cfg))]
        });
    }

    // Retaining the previous assumption
    let mut fpu_present = true;

    if let Some(cpu) = d.cpu.as_ref() {
        let bits = util::unsuffixed(u64::from(cpu.nvic_priority_bits));

        out.extend(quote! {
            ///Number available in the NVIC for configuring priority
            pub const NVIC_PRIO_BITS: u8 = #bits;
        });

        fpu_present = cpu.fpu_present;
    }

    let core_peripherals: &[_] = if fpu_present {
        &[
            "CBP", "CPUID", "DCB", "DWT", "FPB", "FPU", "ITM", "MPU", "NVIC", "SCB", "SYST", "TPIU",
        ]
    } else {
        &[
            "CBP", "CPUID", "DCB", "DWT", "FPB", "ITM", "MPU", "NVIC", "SCB", "SYST", "TPIU",
        ]
    };

    let mut fields = TokenStream::new();
    let mut exprs = TokenStream::new();
    match config.target {
        Target::CortexM => {
            if config.reexport_core_peripherals {
                let fpu = fpu_present.then(|| quote!(FPU,));
                out.extend(quote! {
                    pub use cortex_m::peripheral::Peripherals as CorePeripherals;
                    pub use cortex_m::peripheral::{
                        CBP, CPUID, DCB, DWT, FPB, #fpu ITM, MPU, NVIC, SCB, SYST, TPIU,
                    };
                });
            }

            if config.reexport_interrupt {
                out.extend(quote! {
                    #[cfg(feature = "rt")]
                    pub use cortex_m_rt::interrupt;
                    #[cfg(feature = "rt")]
                    pub use self::Interrupt as interrupt;
                });
            }
        }

        Target::Msp430 => {
            // XXX: Are there any core peripherals, really? Requires bump of msp430 crate.
            // pub use msp430::peripheral::Peripherals as CorePeripherals;
            if config.reexport_interrupt {
                out.extend(quote! {
                    #[cfg(feature = "rt")]
                    pub use msp430_rt::interrupt;
                    #[cfg(feature = "rt")]
                    pub use self::Interrupt as interrupt;
                });
            }
        }

        Target::Mips => {
            if config.reexport_interrupt {
                out.extend(quote! {
                    #[cfg(feature = "rt")]
                    pub use mips_rt::interrupt;
                });
            }
        }

        _ => {}
    }

    let generic_file = include_str!("generic.rs");
    let generic_reg_file = include_str!("generic_reg_vcell.rs");
    let generic_atomic_file = include_str!("generic_atomic.rs");
    let avr_ccp_file = include_str!("generic_avr_ccp.rs");
    if config.generic_mod {
        let mut file = File::create(
            config
                .output_dir
                .as_deref()
                .unwrap_or(Path::new("."))
                .join("generic.rs"),
        )?;
        writeln!(file, "{generic_file}")?;
        writeln!(file, "{generic_reg_file}")?;
        if config.atomics {
            if let Some(atomics_feature) = config.atomics_feature.as_ref() {
                writeln!(file, "#[cfg(feature = \"{atomics_feature}\")]")?;
            }
            writeln!(file, "\n{generic_atomic_file}")?;
        }
        if config.target == Target::Avr {
            writeln!(file, "\n{}", avr_ccp_file)?;
        }

        if !config.make_mod {
            out.extend(quote! {
                #[allow(unused_imports)]
                use generic::*;
                #[doc="Common register and bit access and modify traits"]
                pub mod generic;
            });
        }
    } else {
        let mut tokens = syn::parse_file(generic_file)?.into_token_stream();
        syn::parse_file(generic_reg_file)?.to_tokens(&mut tokens);
        if config.atomics {
            if let Some(atomics_feature) = config.atomics_feature.as_ref() {
                quote!(#[cfg(feature = #atomics_feature)]).to_tokens(&mut tokens);
            }
            syn::parse_file(generic_atomic_file)?.to_tokens(&mut tokens);
        }
        if config.target == Target::Avr {
            syn::parse_file(avr_ccp_file)?.to_tokens(&mut tokens);
        }

        out.extend(quote! {
            #[allow(unused_imports)]
            use generic::*;
            ///Common register and bit access and modify traits
            pub mod generic {
                #tokens
            }
        });
    }

    match config.target {
        Target::RISCV => {
            if config.settings.riscv_config.is_none() {
                warn!("No settings file provided for RISC-V target. Using legacy interrupts rendering");
                warn!("Please, consider migrating your PAC to riscv 0.12.0 or later");
                out.extend(interrupt::render(
                    config.target,
                    &d.peripherals,
                    device_x,
                    config,
                )?);
            } else {
                debug!("Rendering RISC-V specific code");
                out.extend(riscv::render(&d.peripherals, device_x, config)?);
            }
        }
        _ => {
            debug!("Rendering interrupts");
            out.extend(interrupt::render(
                config.target,
                &d.peripherals,
                device_x,
                config,
            )?);
        }
    }

    let feature_format = config.ident_formats.get("peripheral_feature").unwrap();
    for p in &d.peripherals {
        if config.target == Target::CortexM
            && core_peripherals.contains(&p.name.to_uppercase().as_ref())
        {
            // Core peripherals are handled above
            continue;
        }
        if config.target == Target::RISCV && riscv::is_riscv_peripheral(p, &config.settings) {
            // RISC-V specific peripherals are handled above
            continue;
        }

        debug!("Rendering peripheral {}", p.name);
        let periph = peripheral::render(p, &index, config).with_context(|| {
            let group_name = p.group_name.as_deref().unwrap_or("No group name");
            let mut context_string =
                format!("can't render peripheral '{}', group '{group_name}'", p.name);
            if let Some(dname) = p.derived_from.as_ref() {
                context_string += &format!(", derived from: '{dname}'");
            }
            context_string
        })?;

        out.extend(periph);

        if p.registers
            .as_ref()
            .map(|v| &v[..])
            .unwrap_or(&[])
            .is_empty()
            && p.derived_from.is_none()
        {
            // No register block will be generated so don't put this peripheral
            // in the `Peripherals` struct
            continue;
        }
        let mut feature_attribute = TokenStream::new();
        if config.feature_group && p.group_name.is_some() {
            let feature_name = feature_format.apply(p.group_name.as_deref().unwrap());
            feature_attribute.extend(quote! { #[cfg(feature = #feature_name)] })
        };

        let span = Span::call_site();
        match p {
            Peripheral::Single(_p) => {
                let p_name = util::name_of(p, config.ignore_groups);
                let p_feature = feature_format.apply(&p_name);
                let p_ty = ident(&p_name, config, "peripheral", span);
                let p_singleton = ident(&p_name, config, "peripheral_singleton", span);
                if config.feature_peripheral {
                    feature_attribute.extend(quote! { #[cfg(feature = #p_feature)] })
                };
                fields.extend(quote! {
                    #[doc = #p_name]
                    #feature_attribute
                    pub #p_singleton: #p_ty,
                });
                exprs.extend(quote!(#feature_attribute #p_singleton: #p_ty::steal(),));
            }
            Peripheral::Array(p, dim_element) => {
                for p_name in names(p, dim_element) {
                    let p_feature = feature_format.apply(&p_name);
                    let p_ty = ident(&p_name, config, "peripheral", span);
                    let p_singleton = ident(&p_name, config, "peripheral_singleton", span);
                    if config.feature_peripheral {
                        feature_attribute.extend(quote! { #[cfg(feature = #p_feature)] })
                    };
                    fields.extend(quote! {
                        #[doc = #p_name]
                        #feature_attribute
                        pub #p_singleton: #p_ty,
                    });
                    exprs.extend(quote!(#feature_attribute #p_singleton: #p_ty::steal(),));
                }
            }
        }
    }

    out.extend(quote! {
        // NOTE `no_mangle` is used here to prevent linking different minor versions of the device
        // crate as that would let you `take` the device peripherals more than once (one per minor
        // version)
        #[no_mangle]
        static mut DEVICE_PERIPHERALS: bool = false;

        /// All the peripherals.
        #[allow(non_snake_case)]
        pub struct Peripherals {
            #fields
        }

        impl Peripherals {
            /// Returns all the peripherals *once*.
            #[cfg(feature = "critical-section")]
            #[inline]
            pub fn take() -> Option<Self> {
                critical_section::with(|_| {
                    // SAFETY: We are in a critical section, so we have exclusive access
                    // to `DEVICE_PERIPHERALS`.
                    if unsafe { DEVICE_PERIPHERALS } {
                        return None
                    }

                    // SAFETY: `DEVICE_PERIPHERALS` is set to `true` by `Peripherals::steal`,
                    // ensuring the peripherals can only be returned once.
                    Some(unsafe { Peripherals::steal() })
                })
            }

            /// Unchecked version of `Peripherals::take`.
            ///
            /// # Safety
            ///
            /// Each of the returned peripherals must be used at most once.
            #[inline]
            pub unsafe fn steal() -> Self {
                DEVICE_PERIPHERALS = true;

                Peripherals {
                    #exprs
                }
            }
        }
    });

    Ok(out)
}
