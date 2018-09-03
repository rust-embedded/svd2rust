use std::collections::HashMap;
use std::fmt::Write;

use cast::u64;
use quote::Tokens;
use svd::Peripheral;
use syn::Ident;

use errors::*;
use util::{self, ToSanitizedUpperCase};
use Target;

/// Generates code for `src/interrupt.rs`
pub fn render(
    target: &Target,
    peripherals: &[Peripheral],
    device_x: &mut String,
) -> Result<Vec<Tokens>> {
    let interrupts = peripherals
        .iter()
        .flat_map(|p| p.interrupt.iter())
        .map(|i| (i.value, i))
        .collect::<HashMap<_, _>>();

    let mut interrupts = interrupts.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
    interrupts.sort_by_key(|i| i.value);

    let mut root = vec![];
    let mut arms = vec![];
    let mut from_arms = vec![];
    let mut elements = vec![];
    let mut names = vec![];
    let mut variants = vec![];

    // Current position in the vector table
    let mut pos = 0;
    let mut mod_items = vec![];
    for interrupt in &interrupts {
        while pos < interrupt.value {
            elements.push(quote!(Vector { _reserved: 0 }));
            pos += 1;
        }
        pos += 1;

        let name_uc = Ident::new(interrupt.name.to_sanitized_upper_case());
        let description = util::normalize_docstring(format!(
            "{} - {}",
            interrupt.value,
            interrupt
                .description
                .as_ref()
                .unwrap_or(&interrupt.name)
        ));

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

        elements.push(quote!(Vector { _handler: #name_uc }));
        names.push(name_uc);
    }

    let n = util::unsuffixed(u64(pos));
    match *target {
        Target::CortexM => {
            for name in &names {
                writeln!(device_x, "PROVIDE({} = DefaultHandler);" ,name).unwrap();
            }

            root.push(quote! {
                #[cfg(feature = "rt")]
                extern "C" {
                    #(fn #names();)*
                }

                #[doc(hidden)]
                pub union Vector {
                    _handler: unsafe extern "C" fn(),
                    _reserved: u32,
                }

                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #[link_section = ".vector_table.interrupts"]
                #[no_mangle]
                pub static __INTERRUPTS: [Vector; #n] = [
                    #(#elements,)*
                ];

                /// Macro to override a device specific interrupt handler
                ///
                /// # Syntax
                ///
                /// ``` ignore
                /// interrupt!(
                ///     // Name of the interrupt
                ///     $Name:ident,
                ///
                ///     // Path to the interrupt handler (a function)
                ///     $handler:path,
                ///
                ///     // Optional, state preserved across invocations of the handler
                ///     state: $State:ty = $initial_state:expr,
                /// );
                /// ```
                ///
                /// Where `$Name` must match the name of one of the variants of the `Interrupt`
                /// enum.
                ///
                /// The handler must have signature `fn()` is no state was associated to it;
                /// otherwise its signature must be `fn(&mut $State)`.
                #[cfg(feature = "rt")]
                #[macro_export]
                macro_rules! interrupt {
                    ($Name:ident, $handler:path,state: $State:ty = $initial_state:expr) => {
                        #[allow(unsafe_code)]
                        #[deny(private_no_mangle_fns)] // raise an error if this item is not accessible
                        #[no_mangle]
                        pub unsafe extern "C" fn $Name() {
                            static mut STATE: $State = $initial_state;

                            // check that this interrupt exists
                            let _ = $crate::Interrupt::$Name;

                            // validate the signature of the user provided handler
                            let f: fn(&mut $State) = $handler;

                            f(&mut STATE)
                        }
                    };

                    ($Name:ident, $handler:path) => {
                        #[allow(unsafe_code)]
                        #[deny(private_no_mangle_fns)] // raise an error if this item is not accessible
                        #[no_mangle]
                        pub unsafe extern "C" fn $Name() {
                            // check that this interrupt exists
                            let _ = $crate::Interrupt::$Name;

                            // validate the signature of the user provided handler
                            let f: fn() = $handler;

                            f()
                        }
                    };
                }
            });
        }
        Target::Msp430 => {
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

            mod_items.push(quote! {
                #[cfg(feature = "rt")]
                global_asm!("
                DH_TRAMPOLINE:
                    br #DEFAULT_HANDLER
                ");

                #[cfg(feature = "rt")]
                global_asm!(#aliases);

                #[cfg(feature = "rt")]
                extern "msp430-interrupt" {
                    #(fn #names();)*
                }

                #[doc(hidden)]
                pub union Vector {
                    _handler: unsafe extern "msp430-interrupt" fn(),
                    _reserved: u32,
                }

                #[allow(private_no_mangle_statics)]
                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #[link_section = ".vector_table.interrupts"]
                #[no_mangle]
                #[used]
                pub static INTERRUPTS:
                    [Vector; #n] = [
                        #(#elements,)*
                    ];
            });
        }
        Target::RISCV => {}
        Target::None => {}
    }

    let interrupt_enum = quote! {
        /// Enumeration of all the interrupts
        pub enum Interrupt {
            #(#variants)*
        }

        unsafe impl ::bare_metal::Nr for Interrupt {
            #[inline]
            fn nr(&self) -> u8 {
                match *self {
                    #(#arms)*
                }
            }
        }
    };

    if *target == Target::CortexM {
        root.push(interrupt_enum);
    } else {
        mod_items.push(quote! {
            use core::convert::TryFrom;

            #interrupt_enum

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
    }

    if *target != Target::None {
        let abi = match *target {
            Target::Msp430 => "msp430-interrupt",
            _ => "C",
        };

        if *target != Target::CortexM {
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
    }

    if interrupts.len() > 0 {
        root.push(quote! {
            #[doc(hidden)]
            pub mod interrupt {
                #(#mod_items)*
            }
        });

        if *target != Target::CortexM {
            root.push(quote! {
                pub use interrupt::Interrupt;
            });
        }
    }

    Ok(root)
}
