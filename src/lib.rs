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
//! ```
//! $ cargo install svd2rust
//! ```
//!
//! # Usage
//!
//! ```
//! $ svd2rust -i STM32F30x.svd | rustfmt | tee src/lib.rs
//! //! Peripheral access API for STM32F30X microcontrollers (generated using svd2rust v0.4.0)
//!
//! #![deny(missing_docs)]
//! #![deny(warnings)]
//! #![feature(const_fn)]
//! #![no_std]
//!
//! extern crate cortex_m;
//! extern crate vcell;
//!
//! use cortex_m::peripheral::Peripheral;
//!
//! /// Interrupts
//! pub mod interrupt {
//!     ..
//! }
//!
//! /// General-purpose I/Os
//! pub const GPIOA: Peripheral<Gpioa> = unsafe { Peripheral::new(1207959552) };
//!
//! /// General-purpose I/Os
//! pub mod gpioa {
//!     pub struct RegisterBlock {
//!         /// GPIO port mode register
//!         pub moder: Moder,
//!         ..
//!     }
//!     ..
//! }
//!
//! pub use gpioa::RegisterBlock as Gpioa;
//!
//! /// General-purpose I/Os
//! pub const GPIOB: Peripheral<Gpiob> = unsafe { Peripheral::new(1207960576) };
//!
//! /// General-purpose I/Os
//! pub mod gpiob {
//!     ..
//! }
//!
//! pub use gpiob::RegisterBlock as Gpiob;
//!
//! /// GPIOC
//! pub const GPIOC: Peripheral<Gpioc> = unsafe { Peripheral::new(1207961600) };
//!
//! /// Register block
//! pub type Gpioc = Gpiob;
//! ..
//! ```
//!
//! # Dependencies
//!
//! The generated API depends on:
//!
//! - [`cortex-m`](https://crates.io/crates/cortex-m) v0.2.x
//! - [`vcell`](https://crates.io/crates/vcell) v0.1.x
//!
//! # Peripheral API
//!
//! In the root of the generated API, you'll find all the device peripherals as
//! `const`ant `struct`s. You can access the register block behind the
//! peripheral using either of these two methods:
//!
//! - `get()` for `unsafe`, unsynchronized access to the peripheral, or
//!
//! - `borrow()` which grants you exclusive access to the peripheral but can
//!   only be used within a critical section (`interrupt::free`).
//!
//! The register block is basically a `struct` where each field represents a
//! register.
//!
//! ```
//! /// Inter-integrated circuit
//! pub mod i2C1
//!     /// Register block
//!     pub struct RegisterBlock {
//!         /// 0x00 - Control register 1
//!         pub cr1: Cr1,
//!         /// 0x04 - Control register 2
//!         pub cr2: Cr2,
//!         /// 0x08 - Own address register 1
//!         pub oar1: Oar1,
//!         /// 0x0c - Own address register 2
//!         pub oar2: Oar2,
//!         /// 0x10 - Timing register
//!         pub timingr: Timingr,
//!         /// Status register 1
//!         pub timeoutr: Timeoutr,
//!         /// Interrupt and Status register
//!         pub isr: Isr,
//!         /// 0x1c - Interrupt clear register
//!         pub icr: Icr,
//!         /// 0x20 - PEC register
//!         pub pecr: Pecr,
//!         /// 0x24 - Receive data register
//!         pub rxdr: Rxdr,
//!         /// 0x28 - Transmit data register
//!         pub txdr: Txdr,
//!     }
//! }
//! ```
//!
//! # `read` / `modify` / `write` API
//!
//! Each register in the register block, e.g. the `cr1` field in the `I2c`
//! struct, exposes a combination of the `read`, `modify` and `write` methods.
//! Which methods exposes each register depends on whether the register is
//! read-only, read-write or write-only:
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
//! ``` rust
//! impl Cr2 {
//!     /// Modifies the contents of the register
//!     pub fn modify<F>(&mut self, f: F)
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
//!     pub fn write<F>(&mut self, f: F)
//!     where
//!         F: FnOnce(&mut W) -> &mut W,
//!     {
//!         ..
//!     }
//! }
//! ```
//!
//! The `read` method "reads" the register using a **single**, volatile `LDR`
//! instruction and returns a proxy `R` struct that allows access to only the
//! readable bits (i.e. not to the reserved or write-only bits) of the `CR2`
//! register:
//!
//! ``` rust
//! /// Value read from the register
//! impl R {
//!     /// Bit 0 - Slave address bit 0 (master mode)
//!     pub fn sadd0(&self) -> Sadd0R { .. }
//!
//!     /// Bits 1:7 - Slave address bit 7:1 (master mode)
//!     pub fn sadd1(&self) -> Sadd1R { .. }
//!
//!     (..)
//! }
//! ```
//!
//! Usage looks like this:
//!
//! ``` rust
//! // is the SADD0 bit of the CR2 register set?
//! if i2c1.c2r.read().sadd0().bit() {
//!     // yes
//! } else {
//!     // no
//! }
//! ```
//!
//! On the other hand, the `write` method writes some value to the register
//! using a **single**, volatile `STR` instruction. This method involves a `W`
//! struct that only allows constructing valid states of the `CR2` register.
//!
//! The only constructor that `W` provides is `reset_value` which returns the
//! value of the `CR2` register after a reset. The rest of `W` methods are
//! "builder-like" and can be used to modify the writable bitfields of the
//! `CR2` register.
//!
//! ``` rust
//! impl Cr2W {
//!     /// Reset value
//!     pub fn reset_value() -> Self {
//!         Cr2W { bits: 0 }
//!     }
//!
//!     /// Bits 1:7 - Slave address bit 7:1 (master mode)
//!     pub fn sadd1(&mut self) -> _Sadd1W { .. }
//!
//!     /// Bit 0 - Slave address bit 0 (master mode)
//!     pub fn sadd0(&mut self) -> _Sadd0 { .. }
//! }
//! ```
//!
//! The `write` method takes a closure with signature `(&mut W) -> &mut W`. If
//! the "identity closure", `|w| w`, is passed then the `write` method will set
//! the `CR2` register to its reset value. Otherwise, the closure specifies how
//! the reset value will be modified *before* it's written to `CR2`.
//!
//! Usage looks like this:
//!
//! ``` rust
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
//! Finally, the `modify` method performs a **single** read-modify-write
//! operation that involves **one** read (`LDR`) to the register, modifying the
//! value and then a **single** write (`STR`) of the modified value to the
//! register. This method accepts a closure that specifies how the CR2 register
//! will be modified (the `w` argument) and also provides access to the state of
//! the register before it's modified (the `r` argument).
//!
//! Usage looks like this:
//!
//! ``` rust
//! // Set the START bit to 1 while KEEPING the state of the other bits intact
//! i2c1.cr2.modify(|_, w| unsafe { w.start().bit(true) });
//!
//! // TOGGLE the STOP bit, all the other bits will remain untouched
//! i2c1.cr2.modify(|r, w| w.stop().bit(!r.stop().bit()));
//! ```
//!
//! # enumeratedValues
//!
//! If your SVD uses the `<enumeratedValues>` feature, then the API will be
//! *extended* to provide even more type safety. This extension is backward
//! compatible with the original version so you could "upgrade" your SVD by
//! adding, yourself, `<enumeratedValues>` to it and then use `svd2rust` to
//! re-generate a better API that doesn't break the existing code that uses
//! that API.
//!
//! The new `read` API returns an enum that you can match:
//!
//! ```
//! match gpioa.dir.read().pin0() {
//!     gpioa::dir::Pin0R::Input => { .. },
//!     gpioa::dir::Pin0R::Output => { .. },
//! }
//! ```
//!
//! or test for equality
//!
//! ```
//! if gpioa.dir.read().pin0() == gpio::dir::Pin0R::Input {
//!     ..
//! }
//! ```
//!
//! It also provides convenience methods to check for a specific variant without
//! having to import the enum:
//!
//! ```
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
//! ```
//! if gpioa.dir.read().pin0().bits() == 0 {
//!     ..
//! }
//! ```
//!
//! And the new `write` API provides similar additions as well: `variant` lets
//! you pick the value to write from an `enum`eration of the possible ones:
//!
//! ```
//! // enum DirW { Input, Output }
//! gpioa.dir.write(|w| w.pin0().variant(gpio::dir::Pin0W::Output));
//! ```
//!
//! There are convenience methods to pick one of the variants without having to
//! import the enum:
//!
//! ```
//! gpioa.dir.write(|w| w.pin0().output());
//! ```
//!
//! The `bits` (or `bit`) method is still available but will become safe if it's
//! impossible to write a reserved bit pattern into the register:
//!
//! ```
//! // safe because there are only two options: `0` or `1`
//! gpioa.dir.write(|w| w.pin0().bit(true));
//! ```
//!
//! # Interrupt API
//!
//! SVD files also describe the interrupts available to the device. Binary output
//! wise, the interrupt handlers must be stored in the vector table region of
//! Flash memory. `svd2rust` provides an API to easily "register" interrupt
//! handlers.
//!
//! ```
//! /// Interrupts
//! pub mod interrupt {
//!     /// Interrupt handlers
//!     pub struct Handlers {
//!         /// Window Watchdog interrupt
//!         pub wwdg: unsafe extern "C" fn(Wwdg),
//!         /// PVD through EXTI line detection interrupt
//!         pub pvd: unsafe extern "C" fn(Pvd),
//!         ..
//!     }
//!
//!     pub const DEFAULT_HANDLERS: Handlers = Handlers {
//!         wwdg: exception::default_handler,
//!         pvd: exception::default_handler,
//!         ..
//!     };
//! }
//! ```
//!
//! This `Handlers` API then can be used in applications to register the
//! interrupt handlers:
//!
//! ```
//! fn main() { .. }
//!
//! // My interrupt handler
//! extern "C" fn tim7(_: interrupt::Tim7) { .. }
//!
//! #[no_mangle]
//! pub static _INTERRUPT: interrupt::Handlers = interrupt::Handlers {
//!     tim7: tim7, ..interrupt::DEFAULT_HANDLERS
//! }
//! ```
//!
//! This requires some linker script support:
//!
//! ```
//! SECTIONS
//! {
//!     .text ORIGIN(FLASH) :
//!     {
//!         /* Vector table */
//!         LONG(ORIGIN(RAM) + LENGTH(RAM));  /* Initial SP value */
//!         LONG(__reset + 1);  /* Reset handler */
//!
//!         KEEP(*(.rodata._EXCEPTIONS));  /* exception handlers */
//!         KEEP(*(.rodata._INTERRUPTS));  /* interrupt handlers */
//!         ..
//!     }
//!     ..
//! }
//! ```
//!
//! The generated API also includes an `Interrupt` enum
//!
//! ```
//! pub mod interrupt {
//!     /// Enumeration of all the interrupts
//!     pub enum Interrupt {
//!         Wwdg,
//!         Pvd,
//!         ..
//!     }
//! }
//! ```
//!
//! that can be used with `cortex-m`'s NVIC API:
//!
//! ```
//! interrupt::free(|cs| {
//!     NVIC.borrow(&cs).enable(Interrupt::Tim3);
//!     NVIC.borrow(&cs).enable(Interrupt::Tim7);
//! });
//! ```

// NOTE This file is for documentation only
