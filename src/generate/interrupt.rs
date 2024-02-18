use std::collections::HashMap;
use std::fmt::Write;

use crate::svd::Peripheral;
use proc_macro2::{Span, TokenStream};
use quote::quote;

use crate::util::{self, ident};
use crate::{Config, Target};
use anyhow::Result;

/// Generates code for `src/interrupt.rs`
pub fn render(
    target: Target,
    peripherals: &[Peripheral],
    device_x: &mut String,
    config: &Config,
) -> Result<TokenStream> {
    let interrupts = peripherals
        .iter()
        .flat_map(|p| {
            p.interrupt.iter().map(move |i| {
                (i, p.group_name.clone(), {
                    match p {
                        Peripheral::Single(info) => info.name.clone(),
                        Peripheral::Array(info, dim_element) => {
                            svd_rs::array::names(info, dim_element).next().unwrap()
                        }
                    }
                })
            })
        })
        .map(|i| (i.0.value, (i.0, i.1, i.2)))
        .collect::<HashMap<_, _>>();

    let mut interrupts = interrupts.into_values().collect::<Vec<_>>();
    interrupts.sort_by_key(|i| i.0.value);

    let mut root = TokenStream::new();
    let mut from_arms = TokenStream::new();
    let mut elements = TokenStream::new();
    let mut names = vec![];
    let mut names_cfg_attr = vec![];
    let mut variants = TokenStream::new();

    // Current position in the vector table
    let mut pos = 0;
    let mut mod_items = TokenStream::new();
    let span = Span::call_site();
    let feature_format = config.ident_formats.get("peripheral_feature").unwrap();
    for interrupt in &interrupts {
        while pos < interrupt.0.value {
            elements.extend(quote!(Vector { _reserved: 0 },));
            pos += 1;
        }
        pos += 1;

        let i_ty = ident(&interrupt.0.name, config, "interrupt", span);
        let description = format!(
            "{} - {}",
            interrupt.0.value,
            interrupt
                .0
                .description
                .as_ref()
                .map(|s| util::respace(s))
                .as_ref()
                .map(|s| util::escape_special_chars(s))
                .unwrap_or_else(|| interrupt.0.name.clone())
        );

        let value = util::unsuffixed(interrupt.0.value);

        let mut feature_attribute_flag = false;
        let mut feature_attribute = TokenStream::new();
        let mut not_feature_attribute = TokenStream::new();
        if config.feature_group && interrupt.1.is_some() {
            let feature_name = feature_format.apply(interrupt.1.as_ref().unwrap());
            feature_attribute_flag = true;
            feature_attribute.extend(quote! { #[cfg(feature = #feature_name)] });
            not_feature_attribute.extend(quote! { feature = #feature_name, });
        }
        if config.feature_peripheral {
            let feature_name = feature_format.apply(&interrupt.2);
            feature_attribute_flag = true;
            feature_attribute.extend(quote! { #[cfg(feature = #feature_name)] });
            not_feature_attribute.extend(quote! { feature = #feature_name, });
        }
        let not_feature_attribute = quote! { #[cfg(not(all(#not_feature_attribute)))] };

        variants.extend(quote! {
            #[doc = #description]
            #feature_attribute
            #i_ty = #value,
        });

        from_arms.extend(quote! {
            #feature_attribute
            #value => Ok(Interrupt::#i_ty),
        });

        if feature_attribute_flag {
            elements.extend(quote! {
                #not_feature_attribute
                Vector { _reserved: 0 },
                #feature_attribute
                Vector { _handler: #i_ty },
            });
        } else {
            elements.extend(quote!(Vector { _handler: #i_ty },));
        }
        names.push(i_ty);
        names_cfg_attr.push(feature_attribute);
    }

    let n = util::unsuffixed(pos);
    match target {
        Target::CortexM => {
            for name in &names {
                writeln!(device_x, "PROVIDE({name} = DefaultHandler);")?;
            }

            let link_section_name = config
                .interrupt_link_section
                .as_deref()
                .unwrap_or(".vector_table.interrupts");
            let link_section_attr = quote! {
                #[link_section = #link_section_name]
            };

            root.extend(quote! {
                #[cfg(feature = "rt")]
                extern "C" {
                    #(#names_cfg_attr fn #names();)*
                }

                #[doc(hidden)]
                #[repr(C)]
                pub union Vector {
                    _handler: unsafe extern "C" fn(),
                    _reserved: u32,
                }

                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #link_section_attr
                #[no_mangle]
                pub static __INTERRUPTS: [Vector; #n] = [
                    #elements
                ];
            });
        }
        Target::Msp430 => {
            for name in &names {
                writeln!(device_x, "PROVIDE({name} = DefaultHandler);").unwrap();
            }

            let link_section_name = config
                .interrupt_link_section
                .as_deref()
                .unwrap_or(".vector_table.interrupts");
            let link_section_attr = quote! {
                #[link_section = #link_section_name]
            };

            root.extend(quote! {
                #[cfg(feature = "rt")]
                extern "msp430-interrupt" {
                    #(#names_cfg_attr fn #names();)*
                }

                #[doc(hidden)]
                #[repr(C)]
                pub union Vector {
                    _handler: unsafe extern "msp430-interrupt" fn(),
                    _reserved: u16,
                }

                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #link_section_attr
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
                writeln!(device_x, "PROVIDE({name} = DefaultHandler);")?;
            }

            let link_section_attr = config.interrupt_link_section.as_ref().map(|section| {
                quote! {
                    #[link_section = #section]
                }
            });

            root.extend(quote! {
                #[cfg(feature = "rt")]
                extern "C" {
                    #(#names_cfg_attr fn #names();)*
                }

                #[doc(hidden)]
                #[repr(C)]
                pub union Vector {
                    pub _handler: unsafe extern "C" fn(),
                    pub _reserved: usize,
                }

                #[cfg(feature = "rt")]
                #[doc(hidden)]
                #link_section_attr
                #[no_mangle]
                pub static __EXTERNAL_INTERRUPTS: [Vector; #n] = [
                    #elements
                ];
            });
        }
        Target::XtensaLX => {
            for name in &names {
                writeln!(device_x, "PROVIDE({name} = DefaultHandler);")?;
            }

            let link_section_attr = config.interrupt_link_section.as_ref().map(|section| {
                quote! {
                    #[link_section = #section]
                }
            });

            root.extend(quote! {
                #[cfg(feature = "rt")]
                extern "C" {
                    #(#names_cfg_attr fn #names();)*
                }

                #[doc(hidden)]
                #[repr(C)]
                pub union Vector {
                    pub _handler: unsafe extern "C" fn(),
                    _reserved: u32,
                }

                #[cfg(feature = "rt")]
                #link_section_attr
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

    let defmt = config
        .impl_defmt
        .as_ref()
        .map(|feature| quote!(#[cfg_attr(feature = #feature, derive(defmt::Format))]));

    if target == Target::Msp430 {
        let interrupt_enum = quote! {
            ///Enumeration of all the interrupts. This enum is seldom used in application or library crates. It is present primarily for documenting the device's implemented interrupts.
            #defmt
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
            #defmt
            #[derive(Copy, Clone, Debug, PartialEq, Eq)]
            #enum_repr
            pub enum Interrupt {
                #variants
            }
        };

        match target {
            Target::CortexM => {
                root.extend(quote! {
                    #interrupt_enum

                    unsafe impl cortex_m::interrupt::InterruptNumber for Interrupt {
                        #[inline(always)]
                        fn number(#self_token) -> u16 {
                            #nr_expr
                        }
                    }
                });
            }
            Target::XtensaLX => {
                root.extend(quote! {
                    #interrupt_enum

                    unsafe impl xtensa_lx::interrupt::InterruptNumber for Interrupt {
                        #[inline(always)]
                        fn number(#self_token) -> u16 {
                            #nr_expr
                        }
                    }

                    /// TryFromInterruptError
                    #[derive(Debug, Copy, Clone)]
                    pub struct TryFromInterruptError(());

                    impl Interrupt {

                        /// Attempt to convert a given value into an `Interrupt`
                        #[inline]
                        pub fn try_from(value: u16) -> Result<Self, TryFromInterruptError> {
                            match value {
                                #from_arms
                                _ => Err(TryFromInterruptError(())),
                            }
                        }
                    }
                });
            }
            _ => {
                mod_items.extend(quote! {
                    #interrupt_enum

                    /// TryFromInterruptError
                    #[derive(Debug, Copy, Clone)]
                    pub struct TryFromInterruptError(());

                    impl Interrupt {

                        /// Attempt to convert a given value into an `Interrupt`
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
    }

    if target != Target::None {
        let abi = match target {
            Target::Msp430 => "msp430-interrupt",
            _ => "C",
        };

        if target != Target::CortexM
            && target != Target::Msp430
            && target != Target::XtensaLX
            && target != Target::Mips
        {
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

    if !interrupts.is_empty()
        && target != Target::CortexM
        && target != Target::XtensaLX
        && target != Target::Msp430
    {
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
