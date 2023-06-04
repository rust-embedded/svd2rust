//! Peripheral API generator from [CMSIS-SVD] files
//!
//! [CMSIS-SVD]: https://www.keil.com/pack/doc/CMSIS/SVD/html/index.html
//!
//! An SVD file is an XML file that describes the hardware features of a
//! microcontroller. In particular, it lists all the peripherals available to the
//! device, where the registers associated to each device are located in memory,
//! and what's the function of each register.
//!
//! `svd2rust` is a command line tool that transforms SVD files into crates that
//! expose a type safe API to access the peripherals of the device.
//!
//! # Installation
//!
//! ```bash
//! $ cargo install svd2rust
//! ```
//!
//! # Usage
//!
//! `svd2rust` supports Cortex-M, MSP430, RISCV and Xtensa LX6 microcontrollers. The generated crate can
//! be tailored for either architecture using the `--target` flag. The flag accepts "cortex-m",
//! "msp430", "riscv", "xtensa-lx" and "none" as values. "none" can be used to generate a crate that's
//! architecture agnostic and that should work for architectures that `svd2rust` doesn't currently
//! know about like the Cortex-A architecture.
//!
//! If the `--target` flag is omitted `svd2rust` assumes the target is the Cortex-M architecture.
//!
//! If using the `--generic_mod` option, the emitted `generic.rs` needs to be moved to `src`, and
//! [`form`](https://github.com/djmcgill/form) commit fcb397a or newer is required for splitting
//! the emitted `lib.rs`.
//!
//! ## target = cortex-m
//!
//! When targeting the Cortex-M architecture, `svd2rust` will generate three files in the current
//! directory:
//!
//! - `build.rs`, build script that places `device.x` somewhere the linker can find.
//! - `device.x`, linker script that weakly aliases all the interrupt handlers to the default
//! exception handler (`DefaultHandler`).
//! - `lib.rs`, the generated code.
//!
//! All these files must be included in the same device crate. The `lib.rs` file contains several
//! inlined modules and its not formatted. It's recommended to split it out using the [`form`] tool
//! and then format the output using `rustfmt` / `cargo fmt`:
//!
//! [`form`]: https://crates.io/crates/form
//!
//! ``` text
//! $ svd2rust -i STM32F30x.svd
//!
//! $ rm -rf src
//!
//! $ form -i lib.rs -o src/ && rm lib.rs
//!
//! $ cargo fmt
//! ```
//!
//! The resulting crate must provide an opt-in `rt` feature and depend on these crates:
//!
//! - [`critical-section`](https://crates.io/crates/critical-section) v1.x
//! - [`cortex-m`](https://crates.io/crates/cortex-m) >=v0.7.6
//! - [`cortex-m-rt`](https://crates.io/crates/cortex-m-rt) >=v0.6.13
//! - [`vcell`](https://crates.io/crates/vcell) >=v0.1.2
//!
//! Furthermore, the "device" feature of `cortex-m-rt` must be enabled when the `rt` feature
//! is enabled. The `Cargo.toml` of the device crate will look like this:
//!
//! ``` toml
//! [dependencies]
//! critical-section = { version = "1.0", optional = true }
//! cortex-m = "0.7.6"
//! cortex-m-rt = { version = "0.6.13", optional = true }
//! vcell = "0.1.2"
//!
//! [features]
//! rt = ["cortex-m-rt/device"]
//! ```
//!
//! ## target = msp430
//!
//! MSP430 does not natively use the SVD format. However, SVD files can be generated using the
//! [`msp430_svd` application](https://github.com/pftbest/msp430_svd). Most header and DSLite
//! files provided by TI are mirrored in the repository of `msp430_svd`. The application does
//! not need to be installed; the `msp430gen` command below can be substituted by
//! `cargo run -- msp430g2553 > msp430g2553.svd` from the `msp430_svd` crate root.
//!
//! When targeting the MSP430 architecture `svd2rust` will _also_ generate three files in the
//! current directory:
//!
//! - `build.rs`, build script that places `device.x` somewhere the linker can find.
//! - `device.x`, linker script that weakly aliases all the interrupt handlers to the default
//! exception handler (`DefaultHandler`).
//! - `lib.rs`, the generated code.
//!
//! All these files must be included in the same device crate. The `lib.rs` file contains several
//! inlined modules and its not formatted. It's recommend to split it out using the [`form`] tool
//! and then format the output using `rustfmt` / `cargo fmt`:
//!
//! [`form`]: https://crates.io/crates/form
//!
//! ``` text
//! $ msp430gen msp430g2553 > msp430g2553.svd
//!
//! $ xmllint -format msp430g2553.svd --output msp430g2553.svd
//!
//! $ svd2rust -g --target=msp430 -i msp430g2553.svd
//!
//! $ rm -rf src
//!
//! $ form -i lib.rs -o src/ && rm lib.rs
//!
//! $ mv generic.rs src/
//!
//! $ cargo fmt
//! ```
//!
//! The resulting crate must provide opt-in `rt` feature and depend on these crates:
//!
//! - [`critical-section`](https://crates.io/crates/critical-section) v1.x
//! - [`msp430`](https://crates.io/crates/msp430) v0.4.x
//! - [`msp430-rt`](https://crates.io/crates/msp430-rt) v0.4.x
//! - [`vcell`](https://crates.io/crates/vcell) v0.1.x
//!
//! The "device" feature of `msp430-rt` must be enabled when the `rt` feature is
//! enabled. The `Cargo.toml` of the device crate will look like this:
//!
//! ``` toml
//! [dependencies]
//! critical-section = { version = "1.0", optional = true }
//! msp430 = "0.4.0"
//! msp430-rt = { version = "0.4.0", optional = true }
//! vcell = "0.1.0"
//!
//! [features]
//! rt = ["msp430-rt/device"]
//! ```
//!
//! ## Other targets
//!
//! When the target is riscv or none `svd2rust` will emit only the `lib.rs` file. Like in
//! the `cortex-m` case, we recommend you use `form` and `rustfmt` on the output.
//!
//! The resulting crate must provide an opt-in `rt` feature and depend on these crates:
//!
//! - [`critical-section`](https://crates.io/crates/critical-section) v1.x
//! - [`riscv`](https://crates.io/crates/riscv) v0.9.x (if target is RISC-V)
//! - [`riscv-rt`](https://crates.io/crates/riscv-rt) v0.9.x (if target is RISC-V)
//! - [`vcell`](https://crates.io/crates/vcell) v0.1.x
//!
//! The `*-rt` dependencies must be optional only enabled when the `rt` feature is enabled. The
//! `Cargo.toml` of the device crate will look like this for a RISC-V target:
//!
//! ``` toml
//! [dependencies]
//! critical-section = { version = "1.0", optional = true }
//! riscv = "0.9.0"
//! riscv-rt = { version = "0.9.0", optional = true }
//! vcell = "0.1.0"
//!
//! [features]
//! rt = ["riscv-rt"]
//! ```
//!
//! # Peripheral API
//!
//! To use a peripheral first you must get an *instance* of the peripheral. All the device
//! peripherals are modeled as singletons (there can only ever be, at most, one instance of any
//! one of them) and the only way to get an instance of them is through the `Peripherals::take`
//! method, enabled via the `critical-section` feature on the generated crate.
//!
//! ```ignore
//! let mut peripherals = stm32f30x::Peripherals::take().unwrap();
//! peripherals.GPIOA.odr.write(|w| w.bits(1));
//! ```
//!
//! This method can only be successfully called *once* -- that's why the method returns an `Option`.
//! Subsequent calls to the method will result in a `None` value being returned.
//!
//! ```ignore
//! let ok = stm32f30x::Peripherals::take().unwrap();
//! let panics = stm32f30x::Peripherals::take().unwrap();
//! ```
//!
//! This method needs an implementation of `critical-section`. You can implement it yourself or
//! use the implementation provided by the target crate like `cortex-m`, `riscv` and `*-hal` crates.
//! See more details in the [`critical-section`](https://crates.io/crates/critical-section) crate documentation.
//!
//! The singleton property can be *unsafely* bypassed using the `ptr` static method which is
//! available on all the peripheral types. This method is useful for implementing safe higher
//! level abstractions.
//!
//! ```ignore
//! struct PA0 { _0: () }
//! impl PA0 {
//!     fn is_high(&self) -> bool {
//!         // NOTE(unsafe) actually safe because this is an atomic read with no side effects
//!         unsafe { (*GPIOA::ptr()).idr.read().bits() & 1 != 0 }
//!     }
//!
//!     fn is_low(&self) -> bool {
//!         !self.is_high()
//!     }
//! }
//! struct PA1 { _0: () }
//! // ..
//!
//! fn configure(gpioa: GPIOA) -> (PA0, PA1, ..) {
//!     // configure all the PAx pins as inputs
//!     gpioa.moder.reset();
//!     // the GPIOA proxy is destroyed here now the GPIOA register block can't be modified
//!     // thus the configuration of the PAx pins is now frozen
//!     drop(gpioa);
//!     (PA0 { _0: () }, PA1 { _0: () }, ..)
//! }
//! ```
//!
//! Each peripheral proxy `deref`s to a `RegisterBlock` struct that represents a piece of device
//! memory. Each field in this `struct` represents one register in the register block associated to
//! the peripheral.
//!
//! ```ignore
//! /// Inter-integrated circuit
//! pub mod i2c1 {
//!     /// Register block
//!     pub struct RegisterBlock {
//!         /// 0x00 - Control register 1
//!         pub cr1: CR1,
//!         /// 0x04 - Control register 2
//!         pub cr2: CR2,
//!         /// 0x08 - Own address register 1
//!         pub oar1: OAR1,
//!         /// 0x0c - Own address register 2
//!         pub oar2: OAR2,
//!         /// 0x10 - Timing register
//!         pub timingr: TIMINGR,
//!         /// Status register 1
//!         pub timeoutr: TIMEOUTR,
//!         /// Interrupt and Status register
//!         pub isr: ISR,
//!         /// 0x1c - Interrupt clear register
//!         pub icr: ICR,
//!         /// 0x20 - PEC register
//!         pub pecr: PECR,
//!         /// 0x24 - Receive data register
//!         pub rxdr: RXDR,
//!         /// 0x28 - Transmit data register
//!         pub txdr: TXDR,
//!     }
//! }
//! ```
//!
//! # `read` / `modify` / `write` API
//!
//! Each register in the register block, e.g. the `cr1` field in the `I2C` struct, exposes a
//! combination of the `read`, `modify`, and `write` methods. Which method exposes each register
//! depends on whether the register is read-only, read-write or write-only:
//!
//! - read-only registers only expose the `read` method.
//! - write-only registers only expose the `write` method.
//! - read-write registers expose all the methods: `read`, `modify`, and
//!   `write`.
//!
//! Below shows signatures of each of these methods:
//!
//! (using `I2C`'s `CR2` register as an example)
//!
//! ```ignore
//! impl CR2 {
//!     /// Modifies the contents of the register
//!     pub fn modify<F>(&self, f: F)
//!     where
//!         for<'w> F: FnOnce(&R, &'w mut W) -> &'w mut W
//!     {
//!         ..
//!     }
//!
//!     /// Reads the contents of the register
//!     pub fn read(&self) -> R { .. }
//!
//!     /// Writes to the register
//!     pub fn write<F>(&self, f: F)
//!     where
//!         F: FnOnce(&mut W) -> &mut W,
//!     {
//!         ..
//!     }
//! }
//! impl crate::ResetValue for CR2 {
//!     type Type = u32;
//!     fn reset_value() -> Self::Type { 0 }
//! }
//! ```
//!
//! ## `read`
//!
//! The `read` method "reads" the register using a **single**, volatile `LDR` instruction and
//! returns a proxy `R` struct that allows access to only the readable bits (i.e. not to the
//! reserved or write-only bits) of the `CR2` register:
//!
//! ```ignore
//! /// Value read from the register
//! impl R {
//!     /// Bit 0 - Slave address bit 0 (master mode)
//!     pub fn sadd0(&self) -> SADD0_R { .. }
//!
//!     /// Bits 1:7 - Slave address bit 7:1 (master mode)
//!     pub fn sadd1(&self) -> SADD1_R { .. }
//!
//!     (..)
//! }
//! ```
//!
//! Usage looks like this:
//!
//! ```ignore
//! // is the SADD0 bit of the CR2 register set?
//! if i2c1.c2r.read().sadd0().bit() {
//!     // yes
//! } else {
//!     // no
//! }
//! ```
//!
//! ## `reset`
//!
//! The `ResetValue` trait provides `reset_value` which returns the value of the `CR2`
//! register after a reset. This value can be used to modify the
//! writable bitfields of the `CR2` register or reset it to its initial state.
//! Usage looks like this:
//!
//! ```ignore
//! if i2c1.c2r.write().reset()
//! ```
//!
//! ## `write`
//!
//! On the other hand, the `write` method writes some value to the register using a **single**,
//! volatile `STR` instruction. This method involves a `W` struct that only allows constructing
//! valid states of the `CR2` register.
//!
//! ```ignore
//! impl W {
//!     /// Bits 1:7 - Slave address bit 7:1 (master mode)
//!     pub fn sadd1(&mut self) -> SADD1_W { .. }
//!
//!     /// Bit 0 - Slave address bit 0 (master mode)
//!     pub fn sadd0(&mut self) -> SADD0_W { .. }
//! }
//! ```
//!
//! The `write` method takes a closure with signature `(&mut W) -> &mut W`. If the "identity
//! closure", `|w| w`, is passed then the `write` method will set the `CR2` register to its reset
//! value. Otherwise, the closure specifies how the reset value will be modified *before* it's
//! written to `CR2`.
//!
//! Usage looks like this:
//!
//! ```ignore
//! // Starting from the reset value, `0x0000_0000`, change the bitfields SADD0
//! // and SADD1 to `1` and `0b0011110` respectively and write that to the
//! // register CR2.
//! i2c1.cr2.write(|w| unsafe { w.sadd0().bit(true).sadd1().bits(0b0011110) });
//! // NOTE ^ unsafe because you could be writing a reserved bit pattern into
//! // the register. In this case, the SVD doesn't provide enough information to
//! // check whether that's the case.
//!
//! // NOTE The argument to `bits` will be *masked* before writing it to the
//! // bitfield. This makes it impossible to write, for example, `6` to a 2-bit
//! // field; instead, `6 & 3` (i.e. `2`) will be written to the bitfield.
//! ```
//!
//! ## `modify`
//!
//! Finally, the `modify` method performs a **single** read-modify-write
//! operation that involves **one** read (`LDR`) to the register, modifying the
//! value and then a **single** write (`STR`) of the modified value to the
//! register. This method accepts a closure that specifies how the CR2 register
//! will be modified (the `w` argument) and also provides access to the state of
//! the register before it's modified (the `r` argument).
//!
//! Usage looks like this:
//!
//! ```ignore
//! // Set the START bit to 1 while KEEPING the state of the other bits intact
//! i2c1.cr2.modify(|_, w| unsafe { w.start().bit(true) });
//!
//! // TOGGLE the STOP bit, all the other bits will remain untouched
//! i2c1.cr2.modify(|r, w| w.stop().bit(!r.stop().bit()));
//! ```
//!
//! # enumeratedValues
//!
//! If your SVD uses the `<enumeratedValues>` feature, then the API will be *extended* to provide
//! even more type safety. This extension is backward compatible with the original version so you
//! could "upgrade" your SVD by adding, yourself, `<enumeratedValues>` to it and then use `svd2rust`
//! to re-generate a better API that doesn't break the existing code that uses that API.
//!
//! The new `read` API returns an enum that you can match:
//!
//! ```ignore
//! match gpioa.dir.read().pin0().variant() {
//!     gpioa::dir::PIN0_A::Input => { .. },
//!     gpioa::dir::PIN0_A::Output => { .. },
//! }
//! ```
//!
//! or test for equality
//!
//! ```ignore
//! if gpioa.dir.read().pin0().variant() == gpio::dir::PIN0_A::Input {
//!     ..
//! }
//! ```
//!
//! It also provides convenience methods to check for a specific variant without
//! having to import the enum:
//!
//! ```ignore
//! if gpioa.dir.read().pin0().is_input() {
//!     ..
//! }
//!
//! if gpioa.dir.read().pin0().is_output() {
//!     ..
//! }
//! ```
//!
//! The original `bits` method is available as well:
//!
//! ```ignore
//! if gpioa.dir.read().pin0().bits() == 0 {
//!     ..
//! }
//! ```
//!
//! And the new `write` API provides similar additions as well: `variant` lets you pick the value to
//! write from an `enum`eration of the possible ones:
//!
//! ```ignore
//! // enum PIN0_A { Input, Output }
//! gpioa.dir.write(|w| w.pin0().variant(gpio::dir::PIN0_A::Output));
//! ```
//!
//! There are convenience methods to pick one of the variants without having to
//! import the enum:
//!
//! ```ignore
//! gpioa.dir.write(|w| w.pin0().output());
//! ```
//!
//! The `bits` (or `bit`) method is still available but will become safe if it's
//! impossible to write a reserved bit pattern into the register:
//!
//! ```ignore
//! // safe because there are only two options: `0` or `1`
//! gpioa.dir.write(|w| w.pin0().bit(true));
//! ```
//!
//! # Interrupt API
//!
//! SVD files also describe the device interrupts. svd2rust generated crates expose an enumeration
//! of the device interrupts as an `Interrupt` `enum` in the root of the crate. This `enum` can be
//! used with the `cortex-m` crate `NVIC` API.
//!
//! ```ignore
//! use cortex_m::peripheral::Peripherals;
//! use stm32f30x::Interrupt;
//!
//! let p = Peripherals::take().unwrap();
//! let mut nvic = p.NVIC;
//!
//! nvic.enable(Interrupt::TIM2);
//! nvic.enable(Interrupt::TIM3);
//! ```
//!
//! ## the `rt` feature
//!
//! If the `rt` Cargo feature of the svd2rust generated crate is enabled, the crate will populate the
//! part of the vector table that contains the interrupt vectors and provide an
//! [`interrupt!`](macro.interrupt.html) macro (non Cortex-M/MSP430 targets) or [`interrupt`] attribute
//! (Cortex-M or [MSP430](https://docs.rs/msp430-rt-macros/0.1/msp430_rt_macros/attr.interrupt.html))
//! that can be used to register interrupt handlers.
//!
//! [`interrupt`]: https://docs.rs/cortex-m-rt-macros/0.1/cortex_m_rt_macros/attr.interrupt.html
//!
//! ## the `--atomics` flag
//!
//! The `--atomics` flag can be passed to `svd2rust` to extends the register API with operations to
//! atomically set, clear, and toggle specific bits.  The atomic operations allow limited
//! modification of register bits without read-modify-write sequences. As such, they can be
//! concurrently called on different bits in the same register without data races. This flag won't
//! work for RISCV chips without the atomic extension.
//!
//! The `--atomics_feature` flag can also be specified to include atomics implementations conditionally
//! behind the supplied feature name.
//!
//! `portable-atomic` v0.3.16 must be added to the dependencies, with default features off to
//! disable the `fallback` feature.
//!
//! ## the `--impl_debug` flag
//!
//! The `--impl_debug` option will cause svd2rust to generate `core::fmt::Debug` implementations for
//! all registers and blocks.  If a register is readable and has fields defined then each field value
//! will be printed - if no fields are defined then the value of the register will be printed. Any
//! register that has read actions will not be read and printed as `(not read/has read action!)`.
//! Registers that are not readable will have `(write only register)` printed as the value.
//!
//! The `--impl_debug_feature` flag can also be specified to include debug implementations conditionally
//! behind the supplied feature name.
//!
//! Usage examples:
//!
//! ```ignore
//! // These can be called from different contexts even though they are modifying the same register
//! P1.p1out.set_bits(|w| unsafe { w.bits(1 << 1) });
//! P1.p1out.clear_bits(|w| unsafe { w.bits(!(1 << 2)) });
//! P1.p1out.toggle_bits(|w| unsafe { w.bits(1 << 4) });
//! // if impl_debug was used one can print Registers or RegisterBlocks
//! // print single register
//! println!("RTC_CNT {:#?}", unsafe { &*esp32s3::RTC_CNTL::ptr() }.options0);
//! // print all registers for peripheral
//! println!("RTC_CNT {:#?}", unsafe { &*esp32s3::RTC_CNTL::ptr() });
//! ```
#![recursion_limit = "128"]

use quote::quote;
use svd_parser::svd;

pub mod generate;
pub mod util;

pub use crate::util::{Config, Target};

#[non_exhaustive]
pub struct Generation {
    pub lib_rs: String,
    pub device_specific: Option<DeviceSpecific>,
}

#[non_exhaustive]
pub struct DeviceSpecific {
    pub device_x: String,
    pub build_rs: String,
}

use anyhow::{Context, Result};

#[derive(Debug, thiserror::Error)]
pub enum SvdError {
    #[error("Cannot format crate")]
    Fmt,
    #[error("Cannot render SVD device")]
    Render,
}

/// Generates rust code for the specified svd content.
pub fn generate(input: &str, config: &Config) -> Result<Generation> {
    use std::fmt::Write;

    let device = load_from(input, config)?;
    let mut device_x = String::new();
    let items =
        generate::device::render(&device, config, &mut device_x).map_err(|_| SvdError::Render)?;

    let mut lib_rs = String::new();
    writeln!(
        &mut lib_rs,
        "{}",
        quote! {
            #items
        }
    )
    .or(Err(SvdError::Fmt))?;

    let device_specific = if device_x.is_empty() {
        None
    } else {
        Some(DeviceSpecific {
            device_x,
            build_rs: util::build_rs().to_string(),
        })
    };

    Ok(Generation {
        lib_rs,
        device_specific,
    })
}

/// Load a [Device] from a string slice with given [config](crate::util::Config).
pub fn load_from(input: &str, config: &crate::util::Config) -> Result<svd::Device> {
    use self::util::SourceType;
    use svd_parser::ValidateLevel;

    let mut device = match config.source_type {
        SourceType::Xml => {
            let mut parser_config = svd_parser::Config::default();
            parser_config.validate_level = if config.strict {
                ValidateLevel::Strict
            } else {
                ValidateLevel::Weak
            };

            svd_parser::parse_with_config(input, &parser_config)
                .with_context(|| "Error parsing SVD XML file".to_string())?
        }
        #[cfg(feature = "yaml")]
        SourceType::Yaml => serde_yaml::from_str(input)
            .with_context(|| "Error parsing SVD YAML file".to_string())?,
        #[cfg(feature = "json")]
        SourceType::Json => serde_json::from_str(input)
            .with_context(|| "Error parsing SVD JSON file".to_string())?,
    };
    svd_parser::expand_properties(&mut device);
    Ok(device)
}

/// Assigns a handler to an interrupt
///
/// **NOTE** The `interrupt!` macro on Cortex-M and MSP430 device crates is closer in syntax to the
/// [`exception!`] macro. This documentation doesn't apply to it. For the exact syntax of this macro
/// check the documentation of the device crate you are using.
///
/// [`exception!`]: https://docs.rs/cortex-m-rt/0.5.0/cortex_m_rt/macro.exception.html
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
#[macro_export]
macro_rules! interrupt {
    ($NAME:ident, $path:path) => {};
    ($NAME:ident, $path:path, locals: {
        $($lvar:ident: $lty:ty = $lval:expr;)+
    }) => {};
}
