use std::collections::HashMap;

use cast::u64;
use quote::Tokens;
use svd::{Device, Peripheral};
use syn::Ident;

use errors::*;
use util::{self, ToSanitizedUpperCase};
use Target;

/// Generates code for `src/interrupt.rs`
pub fn render(device: &Device, target: &Target, peripherals: &[Peripheral]) -> Result<Vec<Tokens>> {
    let interrupts = peripherals
        .iter()
        .flat_map(|p| p.interrupt.iter())
        .map(|i| (i.value, i))
        .collect::<HashMap<_, _>>();

    let mut interrupts = interrupts.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
    interrupts.sort_by_key(|i| i.value);

    let mut arms = vec![];
    let mut from_arms = vec![];
    let mut elements = vec![];
    let mut names = vec![];
    let mut variants = vec![];

    // Current position in the vector table
    let mut pos = 0;
    let mut mod_items = vec![];
    mod_items.push(quote! {
        use bare_metal::Nr;
    });
    for interrupt in &interrupts {
        while pos < interrupt.value {
            elements.push(quote!(None));
            pos += 1;
        }
        pos += 1;

        let name_uc = Ident::new(interrupt.name.to_sanitized_upper_case());
        let description = format!(
            "{} - {}",
            interrupt.value,
            interrupt
                .description
                .as_ref()
                .map(|s| util::respace(s))
                .unwrap_or_else(|| interrupt.name.clone())
        );

        let value = util::unsuffixed(u64(interrupt.value));

        variants.push(quote! {
            #[doc = #description]
            #name_uc,
        });

        arms.push(quote! {
            Interrupt::#name_uc => #value,
        });

        from_arms.push(quote! {
            #value => Ok(Interrupt::#name_uc),
        });

        elements.push(quote!(Some(#name_uc)));
        names.push(name_uc);
    }

    let aliases = names
        .iter()
        .map(|n| {
            format!(
                "
.weak {0}
{0} = DH_TRAMPOLINE",
                n
            )
        })
        .collect::<Vec<_>>()
        .concat();

    let n = util::unsuffixed(u64(pos));
    match *target {
        Target::CortexM => {
            let is_armv6 = match device.cpu {
                Some(ref cpu) => cpu.name.starts_with("CM0"),
                None => true, // default to armv6 when the <cpu> section is missing
            };

            if is_armv6 {
                // Cortex-M0(+) are ARMv6 and don't have `b.w` (branch with 16 MB range). This
                // can cause linker errors when the handler is too far away. Instead of a small
                // inline assembly shim, we generate a function for those targets and let the
                // compiler do the work (sacrificing a few bytes of code).
                mod_items.push(quote! {
                    #[cfg(feature = "rt")]
                    extern "C" {
                        fn DEFAULT_HANDLER();
                    }

                    #[cfg(feature = "rt")]
                    #[allow(non_snake_case)]
                    #[no_mangle]
                    pub unsafe extern "C" fn DH_TRAMPOLINE() {
                        DEFAULT_HANDLER();
                    }
                });
            } else {
                mod_items.push(quote! {
                    #[cfg(all(target_arch = "arm", feature = "rt"))]
                    global_asm!("
                    .thumb_func
                    DH_TRAMPOLINE:
                        b DEFAULT_HANDLER
                    ");

                    /// Hack to compile on x86
                    #[cfg(all(target_arch = "x86_64", feature = "rt"))]
                    global_asm!("
                    DH_TRAMPOLINE:
                        jmp DEFAULT_HANDLER
                    ");
                })
            }

            mod_items.push(quote! {
                #[cfg(feature = "rt")]
                global_asm!(#aliases);

                #[cfg(feature = "rt")]
                extern "C" {
                    #(fn #names();)*
                }

                #[allow(private_no_mangle_statics)]
                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #[link_section = ".vector_table.interrupts"]
                #[no_mangle]
                #[used]
                pub static INTERRUPTS: [Option<unsafe extern "C" fn()>; #n] = [
                    #(#elements,)*
                ];
            });
        }
        Target::Msp430 => {
            mod_items.push(quote! {
                #[cfg(feature = "rt")]
                global_asm!("
                DH_TRAMPOLINE:
                    jmp DEFAULT_HANDLER
                ");

                #[cfg(feature = "rt")]
                global_asm!(#aliases);

                #[cfg(feature = "rt")]
                extern "msp430-interrupt" {
                    #(fn #names();)*
                }

                #[allow(private_no_mangle_statics)]
                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #[link_section = ".vector_table.interrupts"]
                #[no_mangle]
                #[used]
                pub static INTERRUPTS:
                    [Option<unsafe extern "msp430-interrupt" fn()>; #n] = [
                        #(#elements,)*
                    ];
            });
        }
        Target::RISCV => {}
        Target::None => {}
    }

    mod_items.push(quote! {
        /// Enumeration of all the interrupts
        pub enum Interrupt {
            #(#variants)*
        }

        unsafe impl Nr for Interrupt {
            #[inline]
            fn nr(&self) -> u8 {
                match *self {
                    #(#arms)*
                }
            }
        }

        use core::convert::TryFrom;

        #[derive(Debug, Copy, Clone)]
        pub struct TryFromInterruptError(());

        impl TryFrom<u8> for Interrupt {
            type Error = TryFromInterruptError;

            #[inline]
            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value {
                    #(#from_arms)*
                    _ => Err(TryFromInterruptError(())),
                }
            }
        }
    });

    if *target != Target::None {
        let abi = match *target {
            Target::Msp430 => "msp430-interrupt",
            _ => "C",
        };
        mod_items.push(quote! {
            #[cfg(feature = "rt")]
            #[macro_export]
            macro_rules! interrupt {
                ($NAME:ident, $path:path, locals: {
                    $($lvar:ident:$lty:ty = $lval:expr;)*
                }) => {
                    #[allow(non_snake_case)]
                    mod $NAME {
                        pub struct Locals {
                            $(
                                pub $lvar: $lty,
                            )*
                        }
                    }

                    #[allow(non_snake_case)]
                    #[no_mangle]
                    pub extern #abi fn $NAME() {
                        // check that the handler exists
                        let _ = $crate::interrupt::Interrupt::$NAME;

                        static mut LOCALS: self::$NAME::Locals =
                            self::$NAME::Locals {
                                $(
                                    $lvar: $lval,
                                )*
                            };

                        // type checking
                        let f: fn(&mut self::$NAME::Locals) = $path;
                        f(unsafe { &mut LOCALS });
                    }
                };
                ($NAME:ident, $path:path) => {
                    #[allow(non_snake_case)]
                    #[no_mangle]
                    pub extern #abi fn $NAME() {
                        // check that the handler exists
                        let _ = $crate::interrupt::Interrupt::$NAME;

                        // type checking
                        let f: fn() = $path;
                        f();
                    }
                }
            }
        });
    }

    let mut out = vec![];

    if interrupts.len() > 0 {
        out.push(quote! {
            pub use interrupt::Interrupt;

            #[doc(hidden)]
            pub mod interrupt {
                #(#mod_items)*
            }
        });
    }

    Ok(out)
}
