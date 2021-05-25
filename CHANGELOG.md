# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

### Added

- MSP430 API for atomically changing register bits, gated behind the `--nightly` flag
- New SVD test for `msp430fr2355`
- Option `-o`(`--output-path`) let you specify output directory path

### Changed

- `\n` in descriptions for multiline 
- `_rererved` fields in `RegisterBlock` now hexidemical usize
- options can be set now with `svd2rust.toml` config
- option `ignore_groups` for optional disabling #506
- [breaking-change] move `const_generic` from features to options
- use `Config` to pass options over all render levels
- Use register iterator from `svd-parser`
- rm unneeded `core::convert::` prefix on `From`

### Fixed

- Padding has been corrected for SVD files containing nested array clusters.

  This showed up on Cypress PSOC and Traveo II CPUs.

## [v0.18.0] - 2021-04-17

### Added

- Support for registers with alternateGroup

- New `-m` switch generates a `mod.rs` file instead of `lib.rs`, which can
  be used as a module inside a crate without further modification.

- ESP32/XtensaLX6 support.

- Field array support.

- Add repr(transparent) to Reg struct

- Generated crates now contain the git commit hash and date of svd2rust
  compilation.

- Provide an associated const ptr `PTR` per peripheral RegisterBlock

- Generated peripherals now implement `core::fmt::Debug`.

- Support for MIPS MCU cores, in particular for PIC32MX microcontrollers

### Fixed

- Keyword sanitizing (`async` and unneeded underscores)

- Expand derived clusters.

- Ignore default enumeratedValues.

- Bring `generic` module into scope in `lib.rs` when using `-g` option.

### Changed

- with feature "const-generic" generate const generic variant of
  "field array" structure in addition to structure
  that contain offset (requires rust 1.51) 

- move interrupt generation after generic file

- [breaking-change] make `write_with_zero` method `unsafe` because the way it is

- Use complete path for cluster names

- Rename some generated variables.

- [breaking-change] Publishes the register spec zero-sized type and move all relevant register traits to that struct.

- [breaking-change] Removes the extra type parameter on Reg, making the register spec the sole authority on the shape of the register.

- Wrap register reader/writer and field readers in newtype wrappers, which significantly improves the documentation output.

- Improve documentation on generated registers and fields

- [breaking-change] remove `Variant<U, ENUM_A>`, use `Option<ENUM_A>` instead

- [breaking-change] Update `svd-parser` to `0.11`

- split out register size type (`RawType`) from `ResetValue` trait

- `anyhow` crate is used for error handling

- [breaking-change] Among other cleanups, MSP430 crates are now expected to
  use the `msp430_rt::interrupt` attribute macro and `device.x` for interrupt
  support. The `INTERRUPT` array has been renamed `__INTERRUPT`.

- Documented the nature of the `Interrupt` enum on MSP430 and consequently
  removed all use of `bare-metal` from that architecture

- Don't generate pre Edition 2018 `extern crate` statements anymore

- [breaking-change] Cortex-M PACs now rely on
  `cortex_m::interrupt::InterruptNumber` instead of `bare_metal::Nr` for
  interrupt number handling. The minimum supported `cortex-m` version is now
  **0.7** and `bare-metal` is not a dependency anymore.

### Removed

- Generated use of the register type aliases in favor of directly referencing `Reg<REGISTER_SPEC>`

## [v0.17.0] - 2019-12-31

### Fixed

- Properly use of default RegisterProperties.

### Changed

- Simplified code generation and sped up svd2rust by a some hundred percent
- Represent interrupts directly as `u8` to avoid jump table generation
- Added explicit #[inline] attributes to `Deref` impls
- Enum items now associated with values (C-style), enums annotated with `repr(fty)`
- Bump `svd-parser` dependency (0.9.0)
- Switched from denying all warnings to only a subset.
- Bump logging and CLI arg parsing dependencies

## [v0.16.1] - 2019-08-17

### Fixed

- Handling of missing register description (optional field)

- Improve field enum docs

- Change interrupt vector size for MSP430 to 16 bits from 32 bits

### Changed

- Bump dependencies: `syn`, `quote` and `proc_macro2` v1.0.

## [v0.16.0] - 2019-08-05

### Added

- `variant()` method for field reader and `Variant` enum for fields with reserved values

- Update documentation, add examples for register access methods

- Add `write_with_zero` method for registers without reset value

- command line option `--generic_mod` or `-g` for pushing common
  structures and traits in separate `generic.rs` file

### Changed

- Field readers and writers use one enum where it is possible
  They also were renamed (suffix `_R` for readers, `_W` for writers
  `_A` for common enums, `_AW` if writable variants and readable variants are different)

- Replace register and its reader/writer by generic types `Reg`, `R` and `W`

- Restore `unsafe` marker on register writer `bits()` method

## [v0.15.2] - 2019-07-29

- No changes, just fixing the metadata since crates.io didn't like the keywords

## [v0.15.1] - 2019-07-29

### Added

- Support of 64-bit fields

### Changed

- Modernize `svd2rust-regress`

- Break ultra-long single line output into multiple lines for better usability

- Joined field write proxy into a single line to help dev builds

- Elimated useless 0 shifts to reduce generated code size and fix a clippy lint

- Replace field readers with generic `FR` type

### Fixed

- Correct handling of cluster size tag

## [v0.15.0] - 2019-07-25

- Logging system was introduced by `log` crate.

- `svd2rust` can be used as library.

- `derive_from` now can be used for registers.

- [breaking-change] for access to alternate registers functions now used
  instead of untagged_unions (no more nightly `features`)

- generated code now more compact and compilation faster

- `reset_value` now public const method of register structure

- `Clone`, `Copy`, `Debug`, `PartialEq` implemented for read/write enums

## [v0.14.0] - 2018-12-07

### Added

- On Cortex-M targets the generated code includes a re-export of the
  `cortex_m_rt::interrupt` attribute, but only when the `rt` feature is enabled.

### Changed

- [breaking-change] on non-Cortex targets Interrupt no longer implements the
  `TryFrom` trait; it now provides an inherent `try_from` method.

- [breaking-change] for non-Cortex targets svd2rust no longer outputs the
  generated code to stdout; instead it writes it to a file named `lib.rs`.

- Brackets generated in doc comments are now escaped to prevent warnings on
  nightly where the compiler tries to interpret bracketed content as links to
  structs, enums, etc.

### Fixed

- Some bugs around the generation of unions (see `--nightly` flag).

## [v0.13.1] - 2018-05-16

### Fixed

- Fixed code generation for non Cortex-M targets. `svd2rust` was generating a feature gate with the
wrong name.

- Fixed the example Cargo.toml for msp430 in the documentation.

## [v0.13.0] - 2018-05-12

### Added

- `svd2rust` now emits unions for registers that overlap (have the same address). Before `svd2rust`
  would generate code for only one instance of overlapping registers for memory location. This
  feature requires passing the `--nightly` to `svd2rust` as it generates code that requires a
  nightly compiler to build.

- `svd2rust` now also blacklists the `/` (backslash) and ` ` (space) characters. `svd2rust` removes
  all blacklisted characters from peripheral, register, bitfield and enumeratedValues names.

### Changed

- This crate now compiles on the stable and beta channels.

- [breaking-change] when the target is the cortex-m architecture `svd2rust` generates three files in
  the current directory, instead of dumping the generated code to stdout.

- [breaking-change] the syntax and expansion of the `interrupt!` macro has changed when the target
  is the Cortex-M architecture.

- [breaking-change] the code generated for the Cortex-M architecture now depends on newer versions
  of the bare-metal, cortex-m and cortex-m-rt crates.

- [breaking-change] when the target is the Cortex-M architecture the "rt" feature of the device
  crate must enable the "device" feature of the cortex-m-rt dependency.

### Removed

- [breaking-change] `Interrupt` no longer implements the unstable `TryFrom` trait when the target is
  the Cortex-M architecture.

## [v0.12.1] - 2018-05-06

### Added

- Code generation for `<cluster>`s

- SVD files can now be read from stdin

- RISCV support

### Fixed

- Make the generated code work with recent nightlies by switching from the deprecated
`macro_reexport` feature to the `use_extern_macros` feature, which is planned for stabilization.

- Relocation errors on MSP430

- Code generated for 1-bit enumerated fields

- Handle the case where `dimIndex` information is missing.

- Relocation errors (link errors) on MSP430

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

[Unreleased]: https://github.com/rust-embedded/svd2rust/compare/v0.18.0...HEAD
[v0.18.0]: https://github.com/rust-embedded/svd2rust/compare/v0.17.0...v0.18.0
[v0.17.0]: https://github.com/rust-embedded/svd2rust/compare/v0.16.1...v0.17.0
[v0.16.1]: https://github.com/rust-embedded/svd2rust/compare/v0.16.0...v0.16.1
[v0.16.0]: https://github.com/rust-embedded/svd2rust/compare/v0.15.2...v0.16.0
[v0.15.2]: https://github.com/rust-embedded/svd2rust/compare/v0.15.1...v0.15.2
[v0.15.1]: https://github.com/rust-embedded/svd2rust/compare/v0.15.0...v0.15.1
[v0.15.0]: https://github.com/rust-embedded/svd2rust/compare/v0.14.0...v0.15.0
[v0.14.0]: https://github.com/rust-embedded/svd2rust/compare/v0.13.1...v0.14.0
[v0.13.1]: https://github.com/rust-embedded/svd2rust/compare/v0.13.0...v0.13.1
[v0.13.0]: https://github.com/rust-embedded/svd2rust/compare/v0.12.1...v0.13.0
[v0.12.1]: https://github.com/rust-embedded/svd2rust/compare/v0.12.0...v0.12.1
[v0.12.0]: https://github.com/rust-embedded/svd2rust/compare/v0.11.4...v0.12.0
[v0.11.4]: https://github.com/rust-embedded/svd2rust/compare/v0.11.3...v0.11.4
[v0.11.3]: https://github.com/rust-embedded/svd2rust/compare/v0.11.2...v0.11.3
[v0.11.2]: https://github.com/rust-embedded/svd2rust/compare/v0.11.1...v0.11.2
[v0.11.1]: https://github.com/rust-embedded/svd2rust/compare/v0.11.0...v0.11.1
[v0.11.0]: https://github.com/rust-embedded/svd2rust/compare/v0.10.0...v0.11.0
[v0.10.0]: https://github.com/rust-embedded/svd2rust/compare/v0.9.1...v0.10.0
[v0.9.1]: https://github.com/rust-embedded/svd2rust/compare/v0.9.0...v0.9.1
[v0.9.0]: https://github.com/rust-embedded/svd2rust/compare/v0.8.1...v0.9.0
[v0.8.1]: https://github.com/rust-embedded/svd2rust/compare/v0.8.0...v0.8.1
[v0.8.0]: https://github.com/rust-embedded/svd2rust/compare/v0.7.2...v0.8.0
[v0.7.2]: https://github.com/rust-embedded/svd2rust/compare/v0.7.1...v0.7.2
[v0.7.1]: https://github.com/rust-embedded/svd2rust/compare/v0.7.0...v0.7.1
[v0.7.0]: https://github.com/rust-embedded/svd2rust/compare/v0.6.2...v0.7.0
[v0.6.2]: https://github.com/rust-embedded/svd2rust/compare/v0.6.1...v0.6.2
[v0.6.1]: https://github.com/rust-embedded/svd2rust/compare/v0.6.0...v0.6.1
[v0.6.0]: https://github.com/rust-embedded/svd2rust/compare/v0.5.1...v0.6.0
[v0.5.1]: https://github.com/rust-embedded/svd2rust/compare/v0.5.0...v0.5.1
[v0.5.0]: https://github.com/rust-embedded/svd2rust/compare/v0.4.0...v0.5.0
[v0.4.0]: https://github.com/rust-embedded/svd2rust/compare/v0.3.0...v0.4.0
[v0.3.0]: https://github.com/rust-embedded/svd2rust/compare/v0.2.1...v0.3.0
[v0.2.1]: https://github.com/rust-embedded/svd2rust/compare/v0.2.0...v0.2.1
[v0.2.0]: https://github.com/rust-embedded/svd2rust/compare/v0.1.3...v0.2.0
[v0.1.3]: https://github.com/rust-embedded/svd2rust/compare/v0.1.2...v0.1.3
[v0.1.2]: https://github.com/rust-embedded/svd2rust/compare/v0.1.1...v0.1.2
[v0.1.1]: https://github.com/rust-embedded/svd2rust/compare/v0.1.0...v0.1.1
