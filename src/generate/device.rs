use quote::Tokens;
use svd::Device;
use syn::Ident;

use errors::*;
use util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase};
use Target;

use generate::{interrupt, peripheral};

/// A collection of Tokens and available feature flags
pub struct RenderOutput {
    pub tokens: Vec<Tokens>,
    pub features: Vec<String>,
}

/// Whole device generation
pub fn render(
    d: &Device,
    target: &Target,
    nightly: bool,
    conditional: bool,
    device_x: &mut String,
) -> Result<RenderOutput> {
    let mut output = RenderOutput {
        tokens: vec![],
        features: vec![],
    };

    let doc = format!(
        "Peripheral access API for {0} microcontrollers \
         (generated using svd2rust v{1})\n\n\
         You can find an overview of the API [here].\n\n\
         [here]: https://docs.rs/svd2rust/{1}/svd2rust/#peripheral-api",
        d.name.to_uppercase(),
        env!("CARGO_PKG_VERSION")
    );

    if *target == Target::Msp430 {
        output.tokens.push(quote! {
            #![feature(abi_msp430_interrupt)]
        });
    }

    if *target != Target::None && *target != Target::CortexM {
        output.tokens.push(quote! {
            #![cfg_attr(feature = "rt", feature(global_asm))]
            #![cfg_attr(feature = "rt", feature(use_extern_macros))]
            #![cfg_attr(feature = "rt", feature(used))]
        });
    }

    output.tokens.push(quote! {
        #![doc = #doc]
        #![deny(missing_docs)]
        #![deny(warnings)]
        #![allow(non_camel_case_types)]
        #![no_std]
    });

    if *target != Target::CortexM {
        output.tokens.push(quote! {
            #![feature(const_fn)]
            #![feature(try_from)]
        });
    }

    if nightly {
        output.tokens.push(quote! {
            #![feature(untagged_unions)]
        });
    }

    match *target {
        Target::CortexM => {
            output.tokens.push(quote! {
                extern crate cortex_m;
                #[cfg(feature = "rt")]
                extern crate cortex_m_rt;
            });
        }
        Target::Msp430 => {
            output.tokens.push(quote! {
                extern crate msp430;
                #[cfg(feature = "rt")]
                extern crate msp430_rt;
                #[cfg(feature = "rt")]
                pub use msp430_rt::default_handler;
            });
        }
        Target::RISCV => {
            output.tokens.push(quote! {
                extern crate riscv;
                #[cfg(feature = "rt")]
                extern crate riscv_rt;
            });
        }
        Target::None => {}
    }

    // If conditionals are used, and NO peripherals are selected,
    // certain imports may be unused
    if conditional {
        output.tokens.push(quote! {
            extern crate bare_metal;
            extern crate vcell;

            #[allow(unused_imports)]
            use core::ops::Deref;
            #[allow(unused_imports)]
            use core::marker::PhantomData;
        });
    } else {
        output.tokens.push(quote! {
            extern crate bare_metal;
            extern crate vcell;

            use core::ops::Deref;
            use core::marker::PhantomData;
        });
    }

    // Retaining the previous assumption
    let mut fpu_present = true;

    if let Some(cpu) = d.cpu.as_ref() {
        let bits = util::unsuffixed(cpu.nvic_priority_bits as u64);

        output.tokens.push(quote! {
            /// Number available in the NVIC for configuring priority
            pub const NVIC_PRIO_BITS: u8 = #bits;
        });

        fpu_present = cpu.fpu_present;
    }

    output
        .tokens
        .extend(interrupt::render(target, &d.peripherals, device_x)?);

    let core_peripherals: &[&str];

    if fpu_present {
        core_peripherals = &[
        "CBP", "CPUID", "DCB", "DWT", "FPB", "FPU", "ITM", "MPU", "NVIC", "SCB", "SYST", "TPIU",
        ];
    } else {
        core_peripherals = &[
            "CBP", "CPUID", "DCB", "DWT", "FPB", "ITM", "MPU", "NVIC", "SCB", "SYST", "TPIU"
        ];
    }

    let mut fields = vec![];
    let mut exprs = vec![];
    if *target == Target::CortexM {
        output.tokens.push(quote! {
            pub use cortex_m::peripheral::Peripherals as CorePeripherals;
        });

        if fpu_present {
            output.tokens.push(quote! {
                pub use cortex_m::peripheral::{
                    CBP, CPUID, DCB, DWT, FPB, FPU, ITM, MPU, NVIC, SCB, SYST, TPIU,
                };
            });
        } else {
            output.tokens.push(quote! {
                pub use cortex_m::peripheral::{
                    CBP, CPUID, DCB, DWT, FPB, ITM, MPU, NVIC, SCB, SYST, TPIU,
                };
            });
        }
    }

    for p in &d.peripherals {
        if *target == Target::CortexM && core_peripherals.contains(&&*p.name.to_uppercase()) {
            // Core peripherals are handled above
            continue;
        }

        output
            .tokens
            .extend(peripheral::render(p, &d.peripherals, &d.defaults, nightly, conditional)?);

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

        let upper_name = p.name.to_sanitized_upper_case();
        let snake_name = p.name.to_sanitized_snake_case();
        output.features.push(String::from(snake_name.clone()));
        let id = Ident::new(&*upper_name);

        // Should we allow for conditional compilation of each peripheral?
        if conditional {
            // Yes, annotate each item with a feature gate
            fields.push(quote! {
                #[doc = #upper_name]
                #[cfg(feature = #snake_name)]
                pub #id: #id
            });
            exprs.push(quote!{
                #[cfg(feature = #snake_name)]
                #id: #id { _marker: PhantomData }
            });
        } else {
            // No, all peripherals will always be generated
            fields.push(quote! {
                #[doc = #upper_name]
                pub #id: #id
            });
            exprs.push(quote!{
                #id: #id { _marker: PhantomData }
            });
        }

    }

    let take = match *target {
        Target::CortexM => Some(Ident::new("cortex_m")),
        Target::Msp430 => Some(Ident::new("msp430")),
        Target::RISCV => Some(Ident::new("riscv")),
        Target::None => None,
    }.map(|krate| {
        quote! {
            /// Returns all the peripherals *once*
            #[inline]
            pub fn take() -> Option<Self> {
                #krate::interrupt::free(|_| {
                    if unsafe { DEVICE_PERIPHERALS } {
                        None
                    } else {
                        Some(unsafe { Peripherals::steal() })
                    }
                })
            }
        }
    });

    output.tokens.push(quote! {
        // NOTE `no_mangle` is used here to prevent linking different minor versions of the device
        // crate as that would let you `take` the device peripherals more than once (one per minor
        // version)
        #[allow(renamed_and_removed_lints)]
        // This currently breaks on nightly, to be removed with the line above once 1.31 is stable
        #[allow(private_no_mangle_statics)]
        #[no_mangle]
        static mut DEVICE_PERIPHERALS: bool = false;

        /// All the peripherals
        #[allow(non_snake_case)]
        pub struct Peripherals {
            #(#fields,)*
        }

        impl Peripherals {
            #take

            /// Unchecked version of `Peripherals::take`
            pub unsafe fn steal() -> Self {
                debug_assert!(!DEVICE_PERIPHERALS);

                DEVICE_PERIPHERALS = true;

                Peripherals {
                    #(#exprs,)*
                }
            }
        }
    });

    Ok(output)
}
