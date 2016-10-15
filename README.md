[![Build status](https://travis-ci.org/japaric/svd2rust.svg?branch=master)](https://travis-ci.org/japaric/svd2rust)
[![crates.io](https://img.shields.io/crates/d/svd2rust.svg)](https://crates.io/crates/svd2rust)
[![crates.io](https://img.shields.io/crates/v/svd2rust.svg)](https://crates.io/crates/svd2rust)

# `svd2rust`

> Generate Rust register maps (`struct`s) from SVD files

## Usage

- Get the start address of each peripheral register block.

```
$ svd2rust -i STM32F30x.svd
const GPIOA: usize = 0x48000000;
const GPIOB: usize = 0x48000400;
const GPIOC: usize = 0x48000800;
const GPIOD: usize = 0x48000c00;
const GPIOE: usize = 0x48001000;
const GPIOF: usize = 0x48001400;
(..)
```

- Generate a register map for a single peripheral.

```
$ svd2rust -i STM32F30x.svd rcc | head
#[repr(C)]
/// Reset and clock control
pub struct Rcc {
    /// Clock control register
    pub cr: Cr,
    /// Clock configuration register (RCC_CFGR)
    pub cfgr: Cfgr,
    /// Clock interrupt register (RCC_CIR)
    pub cir: Cir,
    /// APB2 peripheral reset register (RCC_APB2RSTR)
(..)
```

## API

The `svd2rust` generates the following API for each peripheral:

### Register block

A register block "definition" as a `struct`. Example below:

``` rust
/// Inter-integrated circuit
#[repr(C)]
pub struct I2c1 {
    /// 0x00 - Control register 1
    pub cr1: Cr1,
    /// 0x04 - Control register 2
    pub cr2: Cr2,
    /// 0x08 - Own address register 1
    pub oar1: Oar1,
    /// 0x0c - Own address register 2
    pub oar2: Oar2,
    /// 0x10 - Timing register
    pub timingr: Timingr,
    /// 0x14 - Status register 1
    pub timeoutr: Timeoutr,
    /// 0x18 - Interrupt and Status register
    pub isr: Isr,
    /// 0x1c - Interrupt clear register
    pub icr: Icr,
    /// 0x20 - PEC register
    pub pecr: Pecr,
    /// 0x24 - Receive data register
    pub rxdr: Rxdr,
    /// 0x28 - Transmit data register
    pub txdr: Txdr,
}
```

The user has to "instantiate" this definition for each peripheral instance. They have several
choices:

- `static`s and/or `static mut`s. Example below:

``` rust
extern "C" {
    // I2C1 can be accessed in read-write mode
    pub static mut I2C1: I2c;
    // whereas I2C2 can only be accessed in "read-only" mode
    pub static I2C1: I2c;
}
```

Where the addresses of these register blocks must be provided by a linker script:

``` ld
/* layout.ld */
I2C1 = 0x40005400;
I2C2 = 0x40005800;
```

This has the side effect that the `I2C1` and `I2C2` symbols get "taken" so no other C/Rust symbol
(`static`, `function`, etc.) can have the same name.

- "constructor" functions. Example, equivalent to the `static` one, below:

``` rust
// Addresses of the register blocks. These are private.
const I2C1: usize = 0x40005400;
const I2C2: usize = 0x40005800;

// NOTE(unsafe) can alias references to mutable memory
pub unsafe fn i2c1() -> &'mut static I2C {
    unsafe { &mut *(I2C1 as *mut I2c) }
}

pub fn i2c2() -> &'static I2C {
    unsafe { &*(I2C2 as *const I2c) }
}
```

### `read` / `modify` / `write`

Each register in the register block, e.g. the `cr1` field in the `I2c` struct, exposes a combination
of the `read`, `modify` and `write` methods. Which methods exposes each register depends on whether
the register is read-only, read-write or write-only:

- read-only registers only expose the `read` method.
- write-only registers only expose the `write` method.
- read-write registers exposes all the methods: `read`, `modify` and `write`.

This is signature of each of these methods:

(using the `CR2` register as an example)

``` rust
impl Cr2 {
    pub fn modify<F>(&mut self, f: F)
        where for<'w> F: FnOnce(&Cr2R, &'w mut Cr2W) -> &'w mut Cr2W
    {
        ..
    }

    pub fn read(&self) -> Cr2R { .. }

    pub fn write<F>(&mut self, f: F)
        where F: FnOnce(&mut Cr2W) -> &mut Cr2W,
    {
        ..
    }
}
```

The `read` method performs a single, volatile `LDR` instruction and returns a proxy `Cr2R` struct
which allows access to only the readable bits (i.e. not to the reserved bits) of the `CR2` register:

``` rust
impl Cr2R {
    /// Bit 0 - Slave address bit 0 (master mode)
    pub fn sadd0(&self) -> bool { .. }

    /// Bits 1:7 - Slave address bit 7:1 (master mode)
    pub fn sadd1(&self) -> u8 { .. }
    
    (..)
}
```

Usage looks like this:

``` rust
// is the SADD0 bit of the CR2 register set?
if i2c1.c2r.read().sadd0() {
    // something
} else {
    // something else
}
```

The `write` method performs a single, volatile `STR` instruction to write a value to the `CR2`
register. This method involves the `Cr2W` struct which only allows constructing valid states of the
`CR2` register.

The only constructor that `Cr2W` provides is `reset_value` which returns the value of the `CR2`
register after a reset. The rest of `Cr2W` methods are "builder" like and can be used to set or
reset the writable bits of the `CR2` register.

``` rust
impl Cr2W {
    /// Reset value
    pub fn reset_value() -> Self {
        Cr2W { bits: 0 }
    }

    /// Bits 1:7 - Slave address bit 7:1 (master mode)
    pub fn sadd1(&mut self, value: u8) -> &mut Self { .. }

    /// Bit 0 - Slave address bit 0 (master mode)
    pub fn sadd0(&mut self, value: bool) -> &mut Self { .. }
}
```

The `write` method takes a closure with signature `&mut Cr2W -> &mut Cr2W`. If passed the identity
closure, `|w| w`, the `write` method will set the `CR2` register to its reset value. Otherwise, the
closure specifies how that reset value will be modified before it's written to `CR2`.

Usage looks like this:

``` rust
// Write to CR2, its reset value but with its SADD0 and SADD1 fields set to `true` and `0b0011110`
i2c1.cr2.write(|w| w.sadd0(true).sadd1(0b0011110));
```

Finally, the `modify` method performs a read-modify-write operation that involves at least one `LDR`
instruction, one `STR` instruction plus extra instructions to modify the fetched value of the `CR2`
register. This method accepts a closure that specifies how the `CR2` register will be modified.

Usage looks like this:

``` rust
// Toggle the STOP bit of the CR2 register and set the START bit
i2c1.cr2.modify(|r, w| w.stop(!r.stop()).start(true));
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
