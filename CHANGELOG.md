# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

## [v0.12.0] - 2018-01-15

### Changed

- Functions are now marked as `#[inline]` instead of as `#[inline(always)]`.

- Registers specified as arrays in SVD files are now being translated into Rust array in simple
  cases.

- When CPU information is not declared in the SVD file assume that the CPU is an ARMv6-M CPU (the
  lowest common denominator). This only applies when the target is the Cortex-M architecture.

- [breaking-change] Peripherals are now exposed as scoped singletons, instead of as global
  singletons.

## [v0.11.4] - 2017-09-08

### Fixed

- Device crates can be compiled for x86 again.

- Linking issue on ARMv6-M devices.

## [v0.11.3] - 2017-08-01

### Fixed

- Overrides of interrupt handles were being ignored if LTO was not enabled.

## [v0.11.2] - 2017-07-21 - YANKED

### Fixed

- deduplicate non overridden interrupt handlers. This saves 4 bytes of Flash
  memory per interrupt handler.

## [v0.11.1] - 2017-07-07

### Fixed

- the `Peripherals` struct now includes derivedFrom peripherals.

## [v0.11.0] - 2017-07-07

### Added

- Multiarch support. Now `svd2rust` can generate crates for MSP430
  microcontrollers. The target architecture must be specified using the
  `--target` flag.

- The generated crate will now populate the interrupts section of the vector
  table if the "rt" feature is enabled.

- An `interrupt!` macro has been added to the generated crates. This macro can
  be used to override an interrupt handler. This macro is only available if
  the "rt" feature is enabled.

### Changed

- [breaking-change] the generated crates now depend on the [`bare-metal`] crate.

[`bare-metal`]: https://crates.io/crates/bare-metal

- generate crates now have a "rt" Cargo feature. This feature makes the
  generated crate depend on the [`cortex-m-rt`] crate.

[`cortex-m-rt`]: https://crates.io/crates/cortex-m-rt

### Removed

- [breaking-change] the `interrupt` module has been removed from generated
  crates. the `interrupt::Interrupt` enum can now be found at the root of the
  crate.

## [v0.10.0] - 2017-06-11

### Changed

- [breaking-change] the read / write methods on single bits have been renamed
  from `set`, `clear`, `is_set` and `is_clear` to `set_bit`, `clear_bit`,
  `bit_is_set` and `bit_is_clear` respectively. This fixes several collision
  cases where a SVD file named an enumeratedValue BIT (which turns out to be not
  that uncommon!)

## [v0.9.1] - 2017-06-05

### Fixed

- The type of core peripheral register blocks has been changed to uppercase to
  match the device specific ones.

## [v0.9.0] - 2017-06-05 - YANKED

### Changed

- [breaking-change] the types of peripherals, register and bitfields are now
  normalized to uppercase, instead of CamelCase. It was not possible to use
  CamelCase without running into problems like `A_22_5` and `A_2_25` mapping to
  the same identifier `A225`.

### Fixed

- Code generation when the size of register was declared as being 1 bit by the
  SVD file.

## [v0.8.1] - 2017-05-30

### Changed

- The generated crate's documentation now points to svd2rust's documentation
  about the peripheral API.

## [v0.8.0] - 2017-05-29

### Added

- `derivedFrom` between peripherals. That means that `<enumeratedValues
  derivedFrom="peripheral.register.field.enumeratedValue">` will now work.

### Changed

- [breaking-change]. The API of 1-bit fields has been changed to work with
  `bool` instead of with `u8`.

Old API

``` rust
// Read
if peripheral.register.read().field().bits() == 1 { /* something */}

// Write
peripheral.register.write(|w| unsafe { w.field().bits(1) });
```

New API

``` rust
// Read
if peripheral.register.read().field().bit() { /* something */}
// OR
if peripheral.register.read().field().is_set() { /* something */}

// Write. Note that this operation is now safe
peripheral.register.write(|w| w.field().bit(true));
// OR
peripheral.register.write(|w| w.field().set());
```
### Fixed

- Don't generate code for reserved bit-fields as we shouldn't expose an API to
  modify those fields.

## [v0.7.2] - 2017-05-08

### Fixed

- Mark interrupt tokens as `!Send`. This is required to fully fix the memory
  unsafety bug reported in japaric/cortex-m#27.

## [v0.7.1] - 2017-05-07

### Added

- A `.reset()` method, as a shorthand for "write the reset value to this
  register".

### Changed

- Make writing raw bits to a register safe if the SVD indicates so through the
  <WriteConstraint> element.

- Do not reject peripherals without registers.

### Fixed

- Code generation when the SVD file contains no information about interrupts.

## [v0.7.0] - 2017-04-25

### Changed

- [breaking-change]. svd2rust no longer generates an API for core peripherals
  like NVIC. Instead, it just re-exports the cortex-m crate's API. Re-generating
  a crate with this new svd2rust may cause breaking changes in the API of core
  peripherals like NVIC and ITM if and only if the SVD contained information
  about those peripherals in the first place.

## [v0.6.2] - 2017-04-23

### Changed

- `W.bits` is now safe if <WriteConstraint> indicates that it's valid to write
  any value in the full range of the bitfield.

## [v0.6.1] - 2017-04-15

### Fixed

- Add `#[repr(C)]` to the `RegisterBlock` structs. Vanilla structs are not
  guaranteed to preserve the order of their fields as declared now that the
  field reordering optimization has landed.

## [v0.6.0] - 2017-04-11

### Added

- Interrupt tokens now implement the `Nr` trait

### Changed

- [breaking change] the fields of the interrupt::Handlers struct has been
  changed to PascalCase.

## [v0.5.1] - 2017-04-01

### Fixed

- Code generated from SVD files that used enumeratedValues.derivedFrom didn't
  compile.

## [v0.5.0] - 2017-03-27

### Changed

- [breaking change] each peripheral instance now has its own type. Direct use
  of the instances will continue working but function calls whose arguments
  include a peripheral instance will likely break.

## [v0.4.0] - 2017-03-12

### Added

- Support for whole device generation

### Changed

- [breaking-change] The CLI have been totally changed. There's only one option
  now: whole device generation.

## [v0.3.0] - 2017-02-18

### Changed

- The generated API now makes used of the SVD's enumeratedValues information
  if it's available. To make the API that doesn't use enumeratedValues info
  similar to the ones that does use it, the API has significantly changed from
  version

## [v0.2.1] - 2016-12-31

### Added

- Unsafe API to directly modify the bits of a register

## [v0.2.0] - 2016-12-28

### Changed

- [breaking-change] Bitfields named RESERVED are no longer exposed. They were
  causing compilation errors because some registers have more than one bitfield
  named RESERVED. This, in theory, can change the API surface of the generated
  code given the same input SVD but I expect very little code to be affected
  and, actually, those RESERVED bitfields shouldn't have been exposed anyway.

## [v0.1.3] - 2016-12-21

### Added

- Support for "register arrays".

- Support for registers that have no declared "fields".

## [v0.1.2] - 2016-11-27

### Changed

- `svd2rust -i $FILE tim1` will now try to match `tim1`, the name of the
  requested peripheral, *exactly* before looking for a peripheral that start
  with `tim1`. The result is that the previous command now returns the register
  map of TIM1 instead of e.g. the map of TIM15 which appeared "first" in the SVD
  file.

### Fixed

- svd2rust now "sanitizes" register names that match existing Rust keywords.
  This means that if a register is named `mod` in the SVD file, svd2rust will,
  instead, use `mod_` as the name of the register for the generated Rust code.
  With this change, the generated Rust code will compile out of the box, without
  requiring further, manual changes.

- svd2rust no longer assumes that SVD files list the registers of a register
  block sorted by their "offsets". With this change, svd2rust now accepts more
  SVD files.

## [v0.1.1] - 2016-11-13

### Fixed

- Some SVD files specify that two registers exist at the same address.
  `svd2rust` didn't handle this case and panicked. A proper solution to handle
  this case will require `union`s but those have not been stabilized. For now,
  `svd2rust` will simply pick one of the two or more registers that overlap and
  ignore the rest.

## v0.1.0 - 2016-10-15

### Added

- Initial version of the `svd2rust` tool

[Unreleased]: https://github.com/japaric/svd2rust/compare/v0.12.0...HEAD
[v0.12.0]: https://github.com/japaric/svd2rust/compare/v0.11.4...v0.12.0
[v0.11.4]: https://github.com/japaric/svd2rust/compare/v0.11.3...v0.11.4
[v0.11.3]: https://github.com/japaric/svd2rust/compare/v0.11.2...v0.11.3
[v0.11.2]: https://github.com/japaric/svd2rust/compare/v0.11.1...v0.11.2
[v0.11.1]: https://github.com/japaric/svd2rust/compare/v0.11.0...v0.11.1
[v0.11.0]: https://github.com/japaric/svd2rust/compare/v0.10.0...v0.11.0
[v0.10.0]: https://github.com/japaric/svd2rust/compare/v0.9.1...v0.10.0
[v0.9.1]: https://github.com/japaric/svd2rust/compare/v0.9.0...v0.9.1
[v0.9.0]: https://github.com/japaric/svd2rust/compare/v0.8.1...v0.9.0
[v0.8.1]: https://github.com/japaric/svd2rust/compare/v0.8.0...v0.8.1
[v0.8.0]: https://github.com/japaric/svd2rust/compare/v0.7.2...v0.8.0
[v0.7.2]: https://github.com/japaric/svd2rust/compare/v0.7.1...v0.7.2
[v0.7.1]: https://github.com/japaric/svd2rust/compare/v0.7.0...v0.7.1
[v0.7.0]: https://github.com/japaric/svd2rust/compare/v0.6.2...v0.7.0
[v0.6.2]: https://github.com/japaric/svd2rust/compare/v0.6.1...v0.6.2
[v0.6.1]: https://github.com/japaric/svd2rust/compare/v0.6.0...v0.6.1
[v0.6.0]: https://github.com/japaric/svd2rust/compare/v0.5.1...v0.6.0
[v0.5.1]: https://github.com/japaric/svd2rust/compare/v0.5.0...v0.5.1
[v0.5.0]: https://github.com/japaric/svd2rust/compare/v0.4.0...v0.5.0
[v0.4.0]: https://github.com/japaric/svd2rust/compare/v0.3.0...v0.4.0
[v0.3.0]: https://github.com/japaric/svd2rust/compare/v0.2.1...v0.3.0
[v0.2.1]: https://github.com/japaric/svd2rust/compare/v0.2.0...v0.2.1
[v0.2.0]: https://github.com/japaric/svd2rust/compare/v0.1.3...v0.2.0
[v0.1.3]: https://github.com/japaric/svd2rust/compare/v0.1.2...v0.1.3
[v0.1.2]: https://github.com/japaric/svd2rust/compare/v0.1.1...v0.1.2
[v0.1.1]: https://github.com/japaric/svd2rust/compare/v0.1.0...v0.1.1
