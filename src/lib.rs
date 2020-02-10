//! Peripheral API generator from [CMSIS-SVD] files
//!
//! [CMSIS-SVD]: http://www.keil.com/pack/doc/CMSIS/SVD/html/index.html
//!
//! A SVD file is an XML file that describes the hardware features of a
//! microcontroller. In particular, it list all the peripherals available to the
//! device, where the registers associated to each device are located in memory
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
//! `svd2rust` supports Cortex-M, MSP430 and RISCV microcontrollers. The generated crate can be
//! tailored for either architecture using the `--target` flag. The flag accepts "cortex-m",
//! "msp430", "riscv" and "none" as values. "none" can be used to generate a crate that's
//! architecture agnostic and that should work for architectures that `svd2rust` doesn't currently
//! know about like the Cortex-A architecture.
//!
//! If the `--target` flag is omitted `svd2rust` assumes the target is the Cortex-M architecture.
//!
//! ## target = cortex-m
//!
//! When targeting the Cortex-M architecture `svd2rust` will generate three files in the current
//! directory:
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
//! $ svd2rust -i STM32F30x.svd
//!
//! $ rm -rf src
//!
//! $ form -i lib.rs -o src/ && rm lib.rs
//!
//! $ cargo fmt
//! ```
//!
//! The resulting crate must provide an opt-in "rt" feature and depend on these crates:
//! `bare-metal` v0.2.x, `cortex-m` v0.5.x, `cortex-m-rt` >=v0.6.5 and `vcell` v0.1.x. Furthermore
//! the "device" feature of `cortex-m-rt` must be enabled when the "rt" feature is enabled. The
//! `Cargo.toml` of the device crate will look like this:
//!
//! ``` toml
//! [dependencies]
//! bare-metal = "0.2.0"
//! cortex-m = "0.5.8"
//! vcell = "0.1.0"
//!
//! [dependencies.cortex-m-rt]
//! optional = true
//! version = "0.6.5"
//!
//! [features]
//! rt = ["cortex-m-rt/device"]
//! ```
//!
//! ## target = msp430
//!
//! MSP430 does not natively use the SVD format. However, SVD files can be generated using the
//! [`msp430_svd` application](https://github.com/pftbest/msp430_svd). Most header and DSLite
//! files provided by TI are mirrored in the repository of `msp430_svd`.
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
//! $ msp430gen msp430g2553 > out.svd
//!
//! $ xmllint -format out.svd > msp430g2553.svd
//!
//! $ svd2rust --target=msp430 -i msp430g2553.svd
//!
//! $ rm -rf src
//!
//! $ form -i lib.rs -o src/ && rm lib.rs
//!
//! $ cargo fmt
//! ```
//!
//! The resulting crate must provide an opt-in "rt" feature and depend on these crates:
//! `bare-metal` v0.2.x, `msp430` v0.2.x, `msp430-rt` v0.2.x and `vcell` v0.1.x. Furthermore
//! the "device" feature of `msp430-rt` must be enabled when the "rt" feature is enabled. The
//! `Cargo.toml` of the device crate will look like this:
//!
//! ``` toml
//! [dependencies]
//! bare-metal = "0.2.0"
//! msp430 = "0.2.0"
//! vcell = "0.1.0"
//!
//! [dependencies.msp430-rt]
//! optional = true
//! version = "0.2.0"
//!
//! [features]
//! rt = ["msp430-rt/device"]
//! ```
//!
//! ## Other targets
//!
//! When the target is riscv or none `svd2rust` will emit only the `lib.rs` file. Like in
//! the cortex-m case we recommend you use `form` and `rustfmt` on the output.
//!
//! The resulting crate must provide an opt-in "rt" feature and depend on these crates:
//!
//! - [`bare-metal`](https://crates.io/crates/bare-metal) v0.2.x
//! - [`vcell`](https://crates.io/crates/vcell) v0.1.x
//! - [`riscv`](https://crates.io/crates/riscv) v0.4.x if target = riscv.
//! - [`riscv-rt`](https://crates.io/crates/riscv-rt) v0.4.x if target = riscv.
//!
//! The `*-rt` dependencies must be optional only enabled when the "rt" feature is enabled. The
//! `Cargo.toml` of the device crate will look like this for a riscv target:
//!
//! ``` toml
//! [dependencies]
//! bare-metal = "0.2.0"
//! riscv = "0.4.0"
//! vcell = "0.1.0"
//!
//! [dependencies.riscv-rt]
//! optional = true
//! version = "0.4.0"
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
//! method.
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
//! The singleton property can be *unsafely* bypassed using the `ptr` static method which is
//! available on all the peripheral types. This method is a useful for implementing safe higher
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
//! combination of the `read`, `modify` and `write` methods. Which methods exposes each register
//! depends on whether the register is read-only, read-write or write-only:
//!
//! - read-only registers only expose the `read` method.
//! - write-only registers only expose the `write` method.
//! - read-write registers expose all the methods: `read`, `modify` and
//!   `write`.
//!
//! This is signature of each of these methods:
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
//! extern crate cortex_m;
//! extern crate stm32f30x;
//!
//! use cortex_m::interrupt;
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
//! ## the "rt" feature
//!
//! If the "rt" Cargo feature of the svd2rust generated crate is enabled the crate will populate the
//! part of the vector table that contains the interrupt vectors and provide an
//! [`interrupt!`](macro.interrupt.html) macro (non Cortex-M/MSP430 targets) or [`interrupt`] attribute
//! (Cortex-M or [MSP430](https://docs.rs/msp430-rt-macros/0.1/msp430_rt_macros/attr.interrupt.html))
//! that can be used to register interrupt handlers.
//!
//! [`interrupt`]: https://docs.rs/cortex-m-rt-macros/0.1/cortex_m_rt_macros/attr.interrupt.html
//!
//! ## the `--nightly` flag
//!
//! The `--nightly` flag can be passed to `svd2rust` to enable features in the generated api that are only available to a nightly
//! compiler. Currently there are no nightly features the flag is only kept for compatibility with prior versions.
#![recursion_limit = "128"]

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate quote;
use svd_parser as svd;

mod errors;
mod generate;
mod modules;
mod util;

pub use crate::util::Target;

pub struct Generation {
    pub lib_rs: String,
    pub device_specific: Option<DeviceSpecific>,

    // Reserve the right to add more fields to this struct
    _extensible: (),
}

pub struct DeviceSpecific {
    pub device_x: String,
    pub build_rs: String,

    // Reserve the right to add more fields to this struct
    _extensible: (),
}

type Result<T> = std::result::Result<T, SvdError>;
#[derive(Debug)]
pub enum SvdError {
    Fmt,
    Render,
}

/// Generates rust code for the specified svd content.
pub fn generate(xml: &str, target: Target, nightly: bool) -> Result<Generation> {
    use std::fmt::Write;

    let device = svd::parse(xml).unwrap(); //TODO(AJM)
    let mut device_x = String::new();
    let items = generate::device::render(&device, target, nightly, false, &mut device_x)
        .or(Err(SvdError::Render))?
        .items_into_token_stream();

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
            _extensible: (),
        })
    };

    Ok(Generation {
        lib_rs,
        device_specific,
        _extensible: (),
    })
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
