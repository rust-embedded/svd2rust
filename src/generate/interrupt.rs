use std::collections::HashMap;
use std::fmt::Write;

use crate::svd::Peripheral;
use cast::u64;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::util::{self, ToSanitizedUpperCase};
use crate::Target;
use anyhow::Result;

/// Generates code for `src/interrupt.rs`
pub fn render(
    target: Target,
    peripherals: &[Peripheral],
    device_x: &mut String,
) -> Result<TokenStream> {
    let interrupts = peripherals
        .iter()
        .flat_map(|p| p.interrupt.iter())
        .map(|i| (i.value, i))
        .collect::<HashMap<_, _>>();

    let mut interrupts = interrupts.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
    interrupts.sort_by_key(|i| i.value);

    let mut root = TokenStream::new();
    let mut from_arms = TokenStream::new();
    let mut elements = TokenStream::new();
    let mut names = vec![];
    let mut variants = TokenStream::new();

    // Current position in the vector table
    let mut pos = 0;
    let mut mod_items = TokenStream::new();
    for interrupt in &interrupts {
        while pos < interrupt.value {
            elements.extend(quote!(Vector { _reserved: 0 },));
            pos += 1;
        }
        pos += 1;

        let name_uc = Ident::new(&interrupt.name.to_sanitized_upper_case(), Span::call_site());
        let description = format!(
            "{} - {}",
            interrupt.value,
            interrupt
                .description
                .as_ref()
                .map(|s| util::respace(s))
                .as_ref()
                .map(|s| util::escape_brackets(s))
                .unwrap_or_else(|| interrupt.name.clone())
        );

        let value = util::unsuffixed(u64(interrupt.value));

        variants.extend(quote! {
            #[doc = #description]
            #name_uc = #value,
        });

        from_arms.extend(quote! {
            #value => Ok(Interrupt::#name_uc),
        });

        elements.extend(quote!(Vector { _handler: #name_uc },));
        names.push(name_uc);
    }

    let n = util::unsuffixed(u64(pos));
    match target {
        Target::CortexM => {
            for name in &names {
                writeln!(device_x, "PROVIDE({} = DefaultHandler);", name)?;
            }

            root.extend(quote! {
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
                    #elements
                ];
            });
        }
        Target::Msp430 => {
            for name in &names {
                writeln!(device_x, "PROVIDE({} = DefaultHandler);", name).unwrap();
            }

            root.extend(quote! {
                #[cfg(feature = "rt")]
                extern "msp430-interrupt" {
                    #(fn #names();)*
                }

                #[doc(hidden)]
                pub union Vector {
                    _handler: unsafe extern "msp430-interrupt" fn(),
                    _reserved: u16,
                }

                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #[link_section = ".vector_table.interrupts"]
                #[no_mangle]
                #[used]
                pub static __INTERRUPTS:
                    [Vector; #n] = [
                        #elements
                    ];
            });
        }
        Target::RISCV => {
            for name in &names {
                writeln!(device_x, "PROVIDE({} = DefaultHandler);", name)?;
            }

            root.extend(quote! {
                #[cfg(feature = "rt")]
                extern "C" {
                    #(fn #names();)*
                }

                #[doc(hidden)]
                pub union Vector {
                    pub _handler: unsafe extern "C" fn(),
                    pub _reserved: usize,
                }

                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #[no_mangle]
                pub static __EXTERNAL_INTERRUPTS: [Vector; #n] = [
                    #elements
                ];
            });
        }
        Target::XtensaLX => {
            for name in &names {
                writeln!(device_x, "PROVIDE({} = DefaultHandler);", name)?;
            }

            root.extend(quote! {
                #[cfg(feature = "rt")]
                extern "C" {
                    #(fn #names();)*
                }

                #[doc(hidden)]
                pub union Vector {
                    pub _handler: unsafe extern "C" fn(),
                    _reserved: u32,
                }

                #[cfg(feature = "rt")]
                #[doc(hidden)]
                pub static __INTERRUPTS: [Vector; #n] = [
                    #elements
                ];
            });
        }
        Target::Mips => {}
        Target::None => {}
    }

    let self_token = quote!(self);
    let (enum_repr, nr_expr) = if variants.is_empty() {
        (quote!(), quote!(match #self_token {}))
    } else {
        (quote!(#[repr(u16)]), quote!(#self_token as u16))
    };

    if target == Target::Msp430 {
        let interrupt_enum = quote! {
            ///Enumeration of all the interrupts. This enum is seldom used in application or library crates. It is present primarily for documenting the device's implemented interrupts.
            #[derive(Copy, Clone, Debug, PartialEq, Eq)]
            #enum_repr
            pub enum Interrupt {
                #variants
            }
        };

        root.extend(interrupt_enum);
    } else {
        let interrupt_enum = quote! {
            ///Enumeration of all the interrupts.
            #[derive(Copy, Clone, Debug, PartialEq, Eq)]
            #enum_repr
            pub enum Interrupt {
                #variants
            }
        };

        if target == Target::CortexM {
            root.extend(quote! {
                #interrupt_enum

                unsafe impl cortex_m::interrupt::InterruptNumber for Interrupt {
                    #[inline(always)]
                    fn number(#self_token) -> u16 {
                        #nr_expr
                    }
                }
            });
        } else {
            mod_items.extend(quote! {
                #interrupt_enum

                #[derive(Debug, Copy, Clone)]
                pub struct TryFromInterruptError(());

                impl Interrupt {
                    #[inline]
                    pub fn try_from(value: u8) -> Result<Self, TryFromInterruptError> {
                        match value {
                            #from_arms
                            _ => Err(TryFromInterruptError(())),
                        }
                    }
                }
            });
        }
    }

    if target != Target::None {
        let abi = match target {
            Target::Msp430 => "msp430-interrupt",
            _ => "C",
        };

        if target != Target::CortexM && target != Target::Msp430 && target != Target::XtensaLX {
            mod_items.extend(quote! {
                #[cfg(feature = "rt")]
                #[macro_export]
                /// Assigns a handler to an interrupt
                ///
                /// This macro takes two arguments: the name of an interrupt and the path to the
                /// function that will be used as the handler of that interrupt. That function
                /// must have signature `fn()`.
                ///
                /// Optionally, a third argument may be used to declare interrupt local data.
                /// The handler will have exclusive access to these *local* variables on each
                /// invocation. If the third argument is used then the signature of the handler
                /// function must be `fn(&mut $NAME::Locals)` where `$NAME` is the first argument
                /// passed to the macro.
                ///
                /// # Example
                ///
                /// ``` ignore
                /// interrupt!(TIM2, periodic);
                ///
                /// fn periodic() {
                ///     print!(".");
                /// }
                ///
                /// interrupt!(TIM3, tick, locals: {
                ///     tick: bool = false;
                /// });
                ///
                /// fn tick(locals: &mut TIM3::Locals) {
                ///     locals.tick = !locals.tick;
                ///
                ///     if locals.tick {
                ///         println!("Tick");
                ///     } else {
                ///         println!("Tock");
                ///     }
                /// }
                /// ```
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

    if !interrupts.is_empty() && target != Target::CortexM && target != Target::Msp430 {
        root.extend(quote! {
            #[doc(hidden)]
            pub mod interrupt {
                #mod_items
            }
        });

        root.extend(quote! {
            pub use self::interrupt::Interrupt;
        });
    }

    Ok(root)
}
