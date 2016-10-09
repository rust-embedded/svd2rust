[![Build Status][travis]](https://travis-ci.org/japaric/svd2rust)

[travis]: https://travis-ci.org/japaric/svd2rust.svg?branch=master

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
