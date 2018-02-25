use std::collections::HashMap;
use std::io::{self, Write};

use cast::u64;
use quote::{ToTokens, Tokens};
use svd::{Access, BitRange, Defaults, Device, EnumeratedValues, Field, Peripheral, Register,
          Usage, WriteConstraint};
use syn::{self, Ident};

use errors::*;
use util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase, U32Ext, BITS_PER_BYTE};
use Target;

/// Whole device generation
pub fn device(d: &Device, target: &Target, items: &mut Vec<Tokens>) -> Result<()> {
    let doc = format!(
        "Peripheral access API for {0} microcontrollers \
         (generated using svd2rust v{1})\n\n\
         You can find an overview of the API [here].\n\n\
         [here]: https://docs.rs/svd2rust/{1}/svd2rust/#peripheral-api",
        d.name.to_uppercase(),
        env!("CARGO_PKG_VERSION")
    );

    if *target == Target::Msp430 {
        items.push(quote! {
            #![feature(abi_msp430_interrupt)]
        });
    }

    if *target != Target::None {
        items.push(quote! {
            #![cfg_attr(feature = "rt", feature(global_asm))]
            #![cfg_attr(feature = "rt", feature(macro_reexport))]
            #![cfg_attr(feature = "rt", feature(used))]
        });
    }

    items.push(quote! {
        #![doc = #doc]
        #![allow(private_no_mangle_statics)]
        #![deny(missing_docs)]
        #![deny(warnings)]
        #![allow(non_camel_case_types)]
        #![feature(const_fn)]
        #![no_std]
    });

    match *target {
        Target::CortexM => {
            items.push(quote! {
                extern crate cortex_m;
                #[macro_reexport(default_handler, exception)]
                #[cfg(feature = "rt")]
                extern crate cortex_m_rt;
            });
        }
        Target::Msp430 => {
            items.push(quote! {
                extern crate msp430;
                #[macro_reexport(default_handler)]
                #[cfg(feature = "rt")]
                extern crate msp430_rt;
            });
        }
        Target::None => {}
    }

    items.push(quote! {
        extern crate bare_metal;
        extern crate vcell;

        use core::ops::Deref;
        use core::marker::PhantomData;
    });

    if let Some(cpu) = d.cpu.as_ref() {
        let bits = util::unsuffixed(cpu.nvic_priority_bits as u64);

        items.push(quote! {
            /// Number available in the NVIC for configuring priority
            pub const NVIC_PRIO_BITS: u8 = #bits;
        });
    }

    ::generate::interrupt(d, target, &d.peripherals, items);

    const CORE_PERIPHERALS: &[&str] = &[
        "CBP",
        "CPUID",
        "DCB",
        "DWT",
        "FPB",
        "FPU",
        "ITM",
        "MPU",
        "NVIC",
        "SCB",
        "SYST",
        "TPIU",
    ];

    let mut fields = vec![];
    let mut exprs = vec![];
    if *target == Target::CortexM {
        items.push(quote! {
            pub use cortex_m::peripheral::Peripherals as CorePeripherals;
        });

        // NOTE re-export only core peripherals available on *all* Cortex-M devices
        // (if we want to re-export all core peripherals available for the target then we are going
        // to need to replicate the `#[cfg]` stuff that cortex-m uses and that would require all
        // device crates to define the custom `#[cfg]`s that cortex-m uses in their build.rs ...)
        items.push(quote! {
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

        ::generate::peripheral(p, &d.peripherals, items, &d.defaults)?;

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
        Target::None => None,
    }.map(|krate| quote! {
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
    });

    items.push(quote! {
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

    Ok(())
}

/// Generates code for `src/interrupt.rs`
pub fn interrupt(
    device: &Device,
    target: &Target,
    peripherals: &[Peripheral],
    items: &mut Vec<Tokens>,
) {
    let interrupts = peripherals
        .iter()
        .flat_map(|p| p.interrupt.iter())
        .map(|i| (i.value, i))
        .collect::<HashMap<_, _>>();

    let mut interrupts = interrupts.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
    interrupts.sort_by_key(|i| i.value);

    let mut arms = vec![];
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

    if interrupts.len() > 0 {
        items.push(quote! {
            pub use interrupt::Interrupt;

            #[doc(hidden)]
            pub mod interrupt {
                #(#mod_items)*
            }
        });
    }
}

pub fn peripheral(
    p: &Peripheral,
    all_peripherals: &[Peripheral],
    items: &mut Vec<Tokens>,
    defaults: &Defaults,
) -> Result<()> {
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

    items.push(quote! {
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
        return Ok(())
    }

    let registers = p.registers.as_ref().map(|x| x.as_ref()).unwrap_or(&[][..]);

    // No `struct RegisterBlock` can be generated
    if registers.is_empty() {
        // Drop the `pub const` definition of the peripheral
        items.pop();
        return Ok(());
    }

    let mut mod_items = vec![];
    mod_items.push(::generate::register_block(registers, defaults)?);

    for register in registers {
        ::generate::register(
            register,
            registers,
            p,
            all_peripherals,
            defaults,
            &mut mod_items,
        )?;
    }

    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));
    items.push(quote! {
        #[doc = #description]
        pub mod #name_sc {
            use vcell::VolatileCell;

            #(#mod_items)*
        }
    });

    Ok(())
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
                field: util::convert_svd_register(register),
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
                        field: util::convert_svd_register(&register),
                        description: info.description.clone(),
                        offset: info.address_offset,
                        size: register_size * array_info.dim,
                    });
                } else {
                    let mut field_num = 0;
                    for field in util::expand_svd_register(register).iter() {
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

fn unsafety(write_constraint: Option<&WriteConstraint>, width: u32) -> Option<Ident> {
    match write_constraint {
        Some(&WriteConstraint::Range(ref range))
            if range.min as u64 == 0 && range.max as u64 == (1u64 << width) - 1 =>
        {
            // the SVD has acknowledged that it's safe to write
            // any value that can fit in the field
            None
        }
        None if width == 1 => {
            // the field is one bit wide, so we assume it's legal to write
            // either value into it or it wouldn't exist; despite that
            // if a writeConstraint exists then respect it
            None
        }
        _ => Some(Ident::new("unsafe")),
    }
}

pub fn register(
    register: &Register,
    all_registers: &[Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    defs: &Defaults,
    items: &mut Vec<Tokens>,
) -> Result<()> {
    let access = util::access_of(register);
    let name = util::name_of(register);
    let name_pc = Ident::new(&*name.to_sanitized_upper_case());
    let name_sc = Ident::new(&*name.to_sanitized_snake_case());
    let rsize = register
        .size
        .or(defs.size)
        .ok_or_else(|| format!("Register {} has no `size` field", register.name))?;
    let rsize = if rsize < 8 {
        8
    } else if rsize.is_power_of_two() {
        rsize
    } else {
        rsize.next_power_of_two()
    };
    let rty = rsize.to_ty()?;
    let description = util::respace(&register.description);

    let unsafety = unsafety(register.write_constraint.as_ref(), rsize);

    let mut mod_items = vec![];
    let mut reg_impl_items = vec![];
    let mut r_impl_items = vec![];
    let mut w_impl_items = vec![];

    if access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Modifies the contents of the register
            #[inline]
            pub fn modify<F>(&self, f: F)
            where
                for<'w> F: FnOnce(&R, &'w mut W) -> &'w mut W
            {
                let bits = self.register.get();
                let r = R { bits: bits };
                let mut w = W { bits: bits };
                f(&r, &mut w);
                self.register.set(w.bits);
            }
        });
    }

    if access == Access::ReadOnly || access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Reads the contents of the register
            #[inline]
            pub fn read(&self) -> R {
                R { bits: self.register.get() }
            }
        });

        mod_items.push(quote! {
            /// Value read from the register
            pub struct R {
                bits: #rty,
            }
        });

        r_impl_items.push(quote! {
            /// Value of the register as raw bits
            #[inline]
            pub fn bits(&self) -> #rty {
                self.bits
            }
        });
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Writes to the register
            #[inline]
            pub fn write<F>(&self, f: F)
            where
                F: FnOnce(&mut W) -> &mut W
            {
                let mut w = W::reset_value();
                f(&mut w);
                self.register.set(w.bits);
            }
        });

        mod_items.push(quote! {
            /// Value to write to the register
            pub struct W {
                bits: #rty,
            }
        });

        let rv = register
            .reset_value
            .or(defs.reset_value)
            .map(|rv| util::hex(rv))
            .ok_or_else(|| format!("Register {} has no reset value", register.name))?;

        w_impl_items.push(quote! {
            /// Reset value of the register
            #[inline]
            pub fn reset_value() -> W {
                W { bits: #rv }
            }

            /// Writes raw bits to the register
            #[inline]
            pub #unsafety fn bits(&mut self, bits: #rty) -> &mut Self {
                self.bits = bits;
                self
            }
        });
    }

    if access == Access::ReadWrite {
        reg_impl_items.push(quote! {
            /// Writes the reset value to the register
            #[inline]
            pub fn reset(&self) {
                self.write(|w| w)
            }
        })
    }

    mod_items.push(quote! {
        impl super::#name_pc {
            #(#reg_impl_items)*
        }
    });

    if let Some(fields) = register.fields.as_ref() {
        // filter out all reserved fields, as we should not generate code for
        // them
        let fields: Vec<Field> = fields
            .clone()
            .into_iter()
            .filter(|field| field.name.to_lowercase() != "reserved")
            .collect();

        if !fields.is_empty() {
            ::generate::fields(
                &fields,
                register,
                all_registers,
                peripheral,
                all_peripherals,
                &rty,
                access,
                &mut mod_items,
                &mut r_impl_items,
                &mut w_impl_items,
            )?;
        }
    }

    if access == Access::ReadOnly || access == Access::ReadWrite {
        mod_items.push(quote! {
            impl R {
                #(#r_impl_items)*
            }
        });
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {
        mod_items.push(quote! {
            impl W {
                #(#w_impl_items)*
            }
        });
    }

    items.push(quote! {
        #[doc = #description]
        pub struct #name_pc {
            register: VolatileCell<#rty>
        }

        #[doc = #description]
        pub mod #name_sc {
            #(#mod_items)*
        }
    });

    Ok(())
}

pub fn fields(
    fields: &[Field],
    parent: &Register,
    all_registers: &[Register],
    peripheral: &Peripheral,
    all_peripherals: &[Peripheral],
    rty: &Ident,
    access: Access,
    mod_items: &mut Vec<Tokens>,
    r_impl_items: &mut Vec<Tokens>,
    w_impl_items: &mut Vec<Tokens>,
) -> Result<()> {
    struct F<'a> {
        _pc_w: Ident,
        _sc: Ident,
        access: Option<Access>,
        description: String,
        evs: &'a [EnumeratedValues],
        mask: Tokens,
        name: &'a str,
        offset: Tokens,
        pc_r: Ident,
        pc_w: Ident,
        sc: Ident,
        bits: Ident,
        ty: Ident,
        width: u32,
        write_constraint: Option<&'a WriteConstraint>,
    }

    impl<'a> F<'a> {
        fn from(f: &'a Field) -> Result<Self> {
            let BitRange { offset, width } = f.bit_range;
            let sc = f.name.to_sanitized_snake_case();
            let pc = f.name.to_sanitized_upper_case();
            let pc_r = Ident::new(&*format!("{}R", pc));
            let pc_w = Ident::new(&*format!("{}W", pc));
            let _pc_w = Ident::new(&*format!("_{}W", pc));
            let _sc = Ident::new(&*format!("_{}", sc));
            let bits = if width == 1 {
                Ident::new("bit")
            } else {
                Ident::new("bits")
            };
            let mut description = if width == 1 {
                format!("Bit {}", offset)
            } else {
                format!("Bits {}:{}", offset, offset + width - 1)
            };
            if let Some(ref d) = f.description {
                description.push_str(" - ");
                description.push_str(&*util::respace(d));
            }
            Ok(F {
                _pc_w: _pc_w,
                _sc: _sc,
                description: description,
                pc_r: pc_r,
                pc_w: pc_w,
                bits: bits,
                width: width,
                access: f.access,
                evs: &f.enumerated_values,
                sc: Ident::new(&*sc),
                mask: util::hex_or_bool((((1 as u64) << width) - 1) as u32, width),
                name: &f.name,
                offset: util::unsuffixed(u64::from(f.bit_range.offset)),
                ty: width.to_ty()?,
                write_constraint: f.write_constraint.as_ref(),
            })
        }
    }

    let fs = fields.iter().map(F::from).collect::<Result<Vec<_>>>()?;

    // TODO enumeratedValues
    if access == Access::ReadOnly || access == Access::ReadWrite {
        for f in &fs {
            if f.access == Some(Access::WriteOnly) {
                continue;
            }

            let bits = &f.bits;
            let mask = &f.mask;
            let offset = &f.offset;
            let fty = &f.ty;
            let cast = if f.width == 1 {
                quote! { != 0 }
            } else {
                quote! { as #fty }
            };
            let value = quote! {
                const MASK: #fty = #mask;
                const OFFSET: u8 = #offset;

                ((self.bits >> OFFSET) & MASK as #rty) #cast
            };

            if let Some((evs, base)) = util::lookup(
                f.evs,
                fields,
                parent,
                all_registers,
                peripheral,
                all_peripherals,
                Usage::Read,
            )? {
                struct Variant<'a> {
                    description: &'a str,
                    pc: Ident,
                    sc: Ident,
                    value: u64,
                }

                let has_reserved_variant = evs.values.len() != (1 << f.width);
                let variants = evs.values
                    .iter()
                    // filter out all reserved variants, as we should not
                    // generate code for them
                    .filter(|field| field.name.to_lowercase() != "reserved")
                    .map(|ev| {
                        let sc =
                            Ident::new(&*ev.name.to_sanitized_snake_case());
                        let description = ev.description
                            .as_ref()
                            .map(|s| &**s)
                            .unwrap_or("undocumented");

                        let value = u64(ev.value.ok_or_else(|| {
                            format!("EnumeratedValue {} has no <value> field",
                                    ev.name)
                        })?);
                        Ok(Variant {
                            description: description,
                            sc: sc,
                            pc: Ident::new(&*ev.name
                                           .to_sanitized_upper_case()),
                            value: value,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let pc_r = &f.pc_r;
                if let Some(ref base) = base {
                    let pc = base.field.to_sanitized_upper_case();
                    let base_pc_r = Ident::new(&*format!("{}R", pc));
                    let desc = format!("Possible values of the field `{}`", f.name,);

                    if let (Some(ref peripheral), Some(ref register)) =
                        (base.peripheral, base.register)
                    {
                        let pmod_ = peripheral.to_sanitized_snake_case();
                        let rmod_ = register.to_sanitized_snake_case();
                        let pmod_ = Ident::new(&*pmod_);
                        let rmod_ = Ident::new(&*rmod_);

                        mod_items.push(quote! {
                            #[doc = #desc]
                            pub type #pc_r = ::#pmod_::#rmod_::#base_pc_r;
                        });
                    } else if let Some(ref register) = base.register {
                        let mod_ = register.to_sanitized_snake_case();
                        let mod_ = Ident::new(&*mod_);

                        mod_items.push(quote! {
                            #[doc = #desc]
                            pub type #pc_r = super::#mod_::#base_pc_r;
                        });
                    } else {
                        mod_items.push(quote! {
                            #[doc = #desc]
                            pub type #pc_r = #base_pc_r;
                        });
                    }
                }

                let description = &f.description;
                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description]
                    #[inline]
                    pub fn #sc(&self) -> #pc_r {
                        #pc_r::_from({ #value })
                    }
                });

                if base.is_none() {
                    let desc = format!("Possible values of the field `{}`", f.name,);

                    let mut vars = variants
                        .iter()
                        .map(|v| {
                            let desc = v.description;
                            let pc = &v.pc;
                            quote! {
                                #[doc = #desc]
                                #pc
                            }
                        })
                        .collect::<Vec<_>>();
                    if has_reserved_variant {
                        vars.push(quote! {
                            /// Reserved
                            _Reserved(#fty)
                        });
                    }
                    mod_items.push(quote! {
                        #[doc = #desc]
                        #[derive(Clone, Copy, Debug, PartialEq)]
                        pub enum #pc_r {
                            #(#vars),*
                        }
                    });

                    let mut enum_items = vec![];

                    let mut arms = variants
                        .iter()
                        .map(|v| {
                            let value = util::hex_or_bool(v.value as u32, f.width);
                            let pc = &v.pc;

                            quote! {
                                #pc_r::#pc => #value
                            }
                        })
                        .collect::<Vec<_>>();
                    if has_reserved_variant {
                        arms.push(quote! {
                            #pc_r::_Reserved(bits) => bits
                        });
                    }

                    if f.width == 1 {
                        enum_items.push(quote! {
                            /// Returns `true` if the bit is clear (0)
                            #[inline]
                            pub fn bit_is_clear(&self) -> bool {
                                !self.#bits()
                            }

                            /// Returns `true` if the bit is set (1)
                            #[inline]
                            pub fn bit_is_set(&self) -> bool {
                                self.#bits()
                            }
                        });
                    }

                    enum_items.push(quote! {
                        /// Value of the field as raw bits
                        #[inline]
                        pub fn #bits(&self) -> #fty {
                            match *self {
                                #(#arms),*
                            }
                        }
                    });

                    let mut arms = variants
                        .iter()
                        .map(|v| {
                            let i = util::unsuffixed_or_bool(v.value, f.width);
                            let pc = &v.pc;

                            quote! {
                                #i => #pc_r::#pc
                            }
                        })
                        .collect::<Vec<_>>();

                    if has_reserved_variant {
                        arms.push(quote! {
                            i => #pc_r::_Reserved(i)
                        });
                    } else if 1 << f.width.to_ty_width()? != variants.len() {
                        arms.push(quote! {
                            _ => unreachable!()
                        });
                    }

                    enum_items.push(quote! {
                        #[allow(missing_docs)]
                        #[doc(hidden)]
                        #[inline]
                        pub fn _from(value: #fty) -> #pc_r {
                            match value {
                                #(#arms),*,
                            }
                        }
                    });

                    for v in &variants {
                        let pc = &v.pc;
                        let sc = &v.sc;

                        let is_variant = if sc.as_ref().starts_with("_") {
                            Ident::new(&*format!("is{}", sc))
                        } else {
                            Ident::new(&*format!("is_{}", sc))
                        };

                        let doc = format!("Checks if the value of the field is `{}`", pc);
                        enum_items.push(quote! {
                            #[doc = #doc]
                            #[inline]
                            pub fn #is_variant(&self) -> bool {
                                *self == #pc_r::#pc
                            }
                        });
                    }

                    mod_items.push(quote! {
                        impl #pc_r {
                            #(#enum_items)*
                        }
                    });
                }
            } else {
                let description = &f.description;
                let pc_r = &f.pc_r;
                let sc = &f.sc;
                r_impl_items.push(quote! {
                    #[doc = #description]
                    #[inline]
                    pub fn #sc(&self) -> #pc_r {
                        let bits = { #value };
                        #pc_r { bits }
                    }
                });

                let mut pc_r_impl_items = vec![
                    quote! {
                        /// Value of the field as raw bits
                        #[inline]
                        pub fn #bits(&self) -> #fty {
                            self.bits
                        }
                    },
                ];

                if f.width == 1 {
                    pc_r_impl_items.push(quote! {
                        /// Returns `true` if the bit is clear (0)
                        #[inline]
                        pub fn bit_is_clear(&self) -> bool {
                            !self.#bits()
                        }

                        /// Returns `true` if the bit is set (1)
                        #[inline]
                        pub fn bit_is_set(&self) -> bool {
                            self.#bits()
                        }
                    });
                }

                mod_items.push(quote! {
                    /// Value of the field
                    pub struct #pc_r {
                        bits: #fty,
                    }

                    impl #pc_r {
                        #(#pc_r_impl_items)*
                    }
                });
            }
        }
    }

    if access == Access::WriteOnly || access == Access::ReadWrite {
        for f in &fs {
            if f.access == Some(Access::ReadOnly) {
                continue;
            }

            let mut proxy_items = vec![];

            let mut unsafety = unsafety(f.write_constraint, f.width);
            let bits = &f.bits;
            let fty = &f.ty;
            let offset = &f.offset;
            let mask = &f.mask;
            let width = f.width;

            if let Some((evs, base)) = util::lookup(
                &f.evs,
                fields,
                parent,
                all_registers,
                peripheral,
                all_peripherals,
                Usage::Write,
            )? {
                struct Variant {
                    doc: String,
                    pc: Ident,
                    sc: Ident,
                    value: u64,
                }

                let pc_w = &f.pc_w;
                let pc_w_doc = format!("Values that can be written to the field `{}`", f.name);

                let base_pc_w = base.as_ref().map(|base| {
                    let pc = base.field.to_sanitized_upper_case();
                    let base_pc_w = Ident::new(&*format!("{}W", pc));

                    if let (Some(ref peripheral), Some(ref register)) =
                        (base.peripheral, base.register)
                    {
                        let pmod_ = peripheral.to_sanitized_snake_case();
                        let rmod_ = register.to_sanitized_snake_case();
                        let pmod_ = Ident::new(&*pmod_);
                        let rmod_ = Ident::new(&*rmod_);

                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w =
                                ::#pmod_::#rmod_::#base_pc_w;
                        });

                        quote! {
                            ::#pmod_::#rmod_::#base_pc_w
                        }
                    } else if let Some(ref register) = base.register {
                        let mod_ = register.to_sanitized_snake_case();
                        let mod_ = Ident::new(&*mod_);

                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w =
                                super::#mod_::#base_pc_w;
                        });

                        quote! {
                            super::#mod_::#base_pc_w
                        }
                    } else {
                        mod_items.push(quote! {
                            #[doc = #pc_w_doc]
                            pub type #pc_w = #base_pc_w;
                        });

                        quote! {
                            #base_pc_w
                        }
                    }
                });

                let variants = evs.values
                    .iter()
                    // filter out all reserved variants, as we should not
                    // generate code for them
                    .filter(|field| field.name.to_lowercase() != "reserved")
                    .map(
                        |ev| {
                            let value = u64(ev.value.ok_or_else(|| {
                            format!("EnumeratedValue {} has no `<value>` field",
                                    ev.name)})?);

                            Ok(Variant {
                            doc: ev.description
                                .clone()
                                .unwrap_or_else(|| {
                                    format!("`{:b}`", value)
                                }),
                            pc: Ident::new(&*ev.name
                                           .to_sanitized_upper_case()),
                            sc: Ident::new(&*ev.name
                                           .to_sanitized_snake_case()),
                            value: value,
                        })
                        },
                    )
                    .collect::<Result<Vec<_>>>()?;

                if variants.len() == 1 << f.width {
                    unsafety = None;
                }

                if base.is_none() {
                    let variants_pc = variants.iter().map(|v| &v.pc);
                    let variants_doc = variants.iter().map(|v| &*v.doc);
                    mod_items.push(quote! {
                        #[doc = #pc_w_doc]
                        pub enum #pc_w {
                            #(#[doc = #variants_doc]
                            #variants_pc),*
                        }
                    });

                    let arms = variants.iter().map(|v| {
                        let pc = &v.pc;
                        let value = util::unsuffixed_or_bool(v.value, f.width);

                        quote! {
                            #pc_w::#pc => #value
                        }
                    });

                    mod_items.push(quote! {
                        impl #pc_w {
                            #[allow(missing_docs)]
                            #[doc(hidden)]
                            #[inline]
                            pub fn _bits(&self) -> #fty {
                                match *self {
                                    #(#arms),*
                                }
                            }
                        }
                    });
                }


                proxy_items.push(quote! {
                    /// Writes `variant` to the field
                    #[inline]
                    pub fn variant(self, variant: #pc_w) -> &'a mut W {
                        #unsafety {
                            self.#bits(variant._bits())
                        }
                    }
                });

                for v in &variants {
                    let pc = &v.pc;
                    let sc = &v.sc;

                    let doc = util::respace(&v.doc);
                    if let Some(enum_) = base_pc_w.as_ref() {
                        proxy_items.push(quote! {
                            #[doc = #doc]
                            #[inline]
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#enum_::#pc)
                            }
                        });
                    } else {
                        proxy_items.push(quote! {
                            #[doc = #doc]
                            #[inline]
                            pub fn #sc(self) -> &'a mut W {
                                self.variant(#pc_w::#pc)
                            }
                        });
                    }
                }
            }

            if width == 1 {
                proxy_items.push(quote! {
                    /// Sets the field bit
                    pub #unsafety fn set_bit(self) -> &'a mut W {
                        self.bit(true)
                    }

                    /// Clears the field bit
                    pub #unsafety fn clear_bit(self) -> &'a mut W {
                        self.bit(false)
                    }
                });
            }

            proxy_items.push(quote! {
                /// Writes raw bits to the field
                #[inline]
                pub #unsafety fn #bits(self, value: #fty) -> &'a mut W {
                    const MASK: #fty = #mask;
                    const OFFSET: u8 = #offset;

                    self.w.bits &= !((MASK as #rty) << OFFSET);
                    self.w.bits |= ((value & MASK) as #rty) << OFFSET;
                    self.w
                }
            });

            let _pc_w = &f._pc_w;
            mod_items.push(quote! {
                /// Proxy
                pub struct #_pc_w<'a> {
                    w: &'a mut W,
                }

                impl<'a> #_pc_w<'a> {
                    #(#proxy_items)*
                }
            });

            let description = &f.description;
            let sc = &f.sc;
            w_impl_items.push(quote! {
                #[doc = #description]
                #[inline]
                pub fn #sc(&mut self) -> #_pc_w {
                    #_pc_w { w: self }
                }
            })
        }
    }

    Ok(())
}
