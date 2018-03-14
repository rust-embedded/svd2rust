use quote::Tokens;
use svd::Device;
use syn::Ident;

use errors::*;
use util::{self, ToSanitizedUpperCase};
use Target;

use generate::{interrupt, peripheral};

/// Whole device generation
pub fn render(d: &Device, target: &Target) -> Result<Vec<Tokens>> {
    let mut out = vec![];

    let doc = format!(
        "Peripheral access API for {0} microcontrollers \
         (generated using svd2rust v{1})\n\n\
         You can find an overview of the API [here].\n\n\
         [here]: https://docs.rs/svd2rust/{1}/svd2rust/#peripheral-api",
        d.name.to_uppercase(),
        env!("CARGO_PKG_VERSION")
    );

    if *target == Target::Msp430 {
        out.push(quote! {
            #![feature(abi_msp430_interrupt)]
        });
    }

    if *target != Target::None {
        out.push(quote! {
            #![cfg_attr(feature = "rt", feature(global_asm))]
            #![cfg_attr(feature = "rt", feature(use_extern_macros))]
            #![cfg_attr(feature = "rt", feature(used))]
        });
    }

    out.push(quote! {
        #![doc = #doc]
        #![allow(private_no_mangle_statics)]
        #![deny(missing_docs)]
        #![deny(warnings)]
        #![allow(non_camel_case_types)]
        #![feature(const_fn)]
        #![feature(try_from)]
        #![feature(untagged_unions)]
        #![no_std]
    });

    match *target {
        Target::CortexM => {
            out.push(quote! {
                extern crate cortex_m;
                #[cfg(feature = "rt")]
                extern crate cortex_m_rt;
                #[cfg(feature = "rt")]
                pub use cortex_m_rt::{default_handler, exception};
            });
        }
        Target::Msp430 => {
            out.push(quote! {
                extern crate msp430;
                #[cfg(feature = "rt")]
                extern crate msp430_rt;
                #[cfg(feature = "rt")]
                pub use msp430_rt::default_handler;
            });
        }
        Target::RISCV => {
            out.push(quote! {
                extern crate riscv;
                #[cfg(feature = "rt")]
                extern crate riscv_rt;
            });
        }
        Target::None => {}
    }

    out.push(quote! {
        extern crate bare_metal;
        extern crate vcell;

        use core::ops::Deref;
        use core::marker::PhantomData;
    });

    if let Some(cpu) = d.cpu.as_ref() {
        let bits = util::unsuffixed(cpu.nvic_priority_bits as u64);

        out.push(quote! {
            /// Number available in the NVIC for configuring priority
            pub const NVIC_PRIO_BITS: u8 = #bits;
        });
    }

    out.extend(interrupt::render(d, target, &d.peripherals)?);

    const CORE_PERIPHERALS: &[&str] = &[
        "CBP", "CPUID", "DCB", "DWT", "FPB", "FPU", "ITM", "MPU", "NVIC", "SCB", "SYST", "TPIU"
    ];

    let mut fields = vec![];
    let mut exprs = vec![];
    if *target == Target::CortexM {
        out.push(quote! {
            pub use cortex_m::peripheral::Peripherals as CorePeripherals;
        });

        // NOTE re-export only core peripherals available on *all* Cortex-M devices
        // (if we want to re-export all core peripherals available for the target then we are going
        // to need to replicate the `#[cfg]` stuff that cortex-m uses and that would require all
        // device crates to define the custom `#[cfg]`s that cortex-m uses in their build.rs ...)
        out.push(quote! {
            pub use cortex_m::peripheral::CPUID;
            pub use cortex_m::peripheral::DCB;
            pub use cortex_m::peripheral::DWT;
            pub use cortex_m::peripheral::MPU;
            pub use cortex_m::peripheral::NVIC;
            pub use cortex_m::peripheral::SCB;
            pub use cortex_m::peripheral::SYST;
        });
    }

    for p in &d.peripherals {
        if *target == Target::CortexM && CORE_PERIPHERALS.contains(&&*p.name.to_uppercase()) {
            // Core peripherals are handled above
            continue;
        }


        out.extend(peripheral::render(p, &d.peripherals, &d.defaults)?);

        if p.registers
            .as_ref()
            .map(|v| &v[..])
            .unwrap_or(&[])
            .is_empty() && p.derived_from.is_none()
        {
            // No register block will be generated so don't put this peripheral
            // in the `Peripherals` struct
            continue;
        }

        let p = p.name.to_sanitized_upper_case();
        let id = Ident::new(&*p);
        fields.push(quote! {
            #[doc = #p]
            pub #id: #id
        });
        exprs.push(quote!(#id: #id { _marker: PhantomData }));
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

    out.push(quote! {
        // NOTE `no_mangle` is used here to prevent linking different minor versions of the device
        // crate as that would let you `take` the device peripherals more than once (one per minor
        // version)
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

    Ok(out)
}
