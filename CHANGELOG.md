# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/)
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

- Fix STM32-patched CI
- Fix `enumeratedValues` with `isDefault` only
- Fix invalid `Punct` error from `proc_macro2`

## [v0.33.4] - 2024-06-16

- Add `html-url` option to access `svdtools html` files from docs
- Move `Reg` in separate file
- Use `warning` class in docs
- Refactor `Accessor`

## [v0.33.3] - 2024-05-10

- Yet more clean field & register `Debug`

## [v0.33.2] - 2024-05-07

- Remove unneeded `format_args` in register `Debug` impl

## [v0.33.1] - 2024-04-20

- Add checked `set` for not full safe fields

## [v0.33.0] - 2024-03-26

- Add `IsEnum` constraint for `FieldWriter`s (fix `variant` safety)
- Make field writer `bits` always `unsafe`, add `set` for safe writing
- Fix bit writer type for `ModifiedWriteValues::ZeroToSet`

## [v0.32.0] - 2024-02-26

- Bump MSRV to 1.74
- generic unsafe `W::bits` + safe `W::set`
- Add `base-address-shift` config flag
- Use `PascalCase` for type idents, fix case changing bugs, add `--ident-format` (`-f`) option flag
- Add `enum_read_name` for `read-only` enums, `RWEnum` helper
- Reexport enums inside register again
- Add `DimSuffix` helper trait

## [v0.31.5] - 2024-01-04

- `move` in `RegisterBlock::reg_iter` implementation (iterator of register/cluster array)
- Fix `cargo doc` constants generation

## [v0.31.4] - 2024-01-03

- Custom prefix/case/suffix for identifiers (by `svd2rust.toml` config file)

## [v0.31.3] - 2023-12-25

- Add `svd::Device` validation after parsing by `serde`
- Add `skip-crate-attributes` config flag
- Better display parsing errors
- `move` in `R::field_iter` implementation (iterator of field array values)

## [v0.31.2] - 2023-11-29

- Add iterators for register/cluster/field arrays
- Use parentheses instead of square brackets in docs for field arrays

## [v0.31.1] - 2023-11-27

- Fix cluster arrays
- Remove needless reference in `ArrayElemAccessor`

## [v0.31.0] - 2023-11-24

- Use methods to access any register or cluster
- Remove all deny lints from generated crate
- Add `reexport-core-peripherals` and `reexport-interrupt` features disabled by default
- remove `ArrayProxy` and `const_generic` feature
- `FieldWriter` takes offset as struct field instead of const generic.
  Improves SVD field array access
  Add `width`, `offset` methods
- *breaking change* Always numerates field arrays from 0
- Support of default value for `EnumeratedValues`
- move `Config` to `config` module
- add `impl-defmt` config flag
- Use dash instead of underscore in flag names

## [v0.30.3] - 2023-11-19

- Remove unstable lints
- Mark `Vector` union as `repr(C)`
- Support `dimArrayIndex` for array names and descriptions

## [v0.30.2] - 2023-10-22

- Fix documentation warnings
- Use `ArrayProxy` for memory disjoined register arrays
- Use `const fn` where allowed

## [v0.30.1] - 2023-10-01

- Fix clippy lints on `nightly`
- Bump MSRV to 1.70
- Fix `derivedFrom` on field

## [v0.30.0] - 2023-08-16

- Add `aarch64` target for releases, more readme badges
- Fix when `atomics` features is generated but not enabled
- move hidden structs into module, add register reader/writer links into `SPEC` docs (#736)
- removed register writer & reader wrappers, generic `REG` in field writers (#731)
- Updated syn to version 2 (#732)
- Let readable field fetch doc from svd description (#734)
- Add `steal()` for each peripheral

## [v0.29.0] - 2023-06-05

- `FieldFpec` instead or `fty` generic (#722)
- print error on ci `curl` request fail (#725)
- removed `rty` generic in `FieldWriter` (#721)
- `bool` and `u8` as default generics for `BitReader/Writer` and `FieldReader/Writer` (#720)
- Bump MSRV to 1.65 (#711)
- Optimize case change/sanitize (#715)
- Fix dangling implicit derives (#703)
- Fix escaping <> and & characters in doc attributes (#711)
- Add `interrupt_link_section` config parameter for controlling the `#[link_section = "..."]` attribute of `__INTERRUPTS` (#718)
- Add option to implement Debug for readable registers (#716)
- Add `atomics-feature` (#729)

## [v0.28.0] - 2022-12-25

- Generate atomic register code for non-MSP430 targets
- Change --nightly flag to --atomics
- Add handling for disjoint register arrays and validation of derives

## [v0.27.2] - 2022-11-06

- mark alternate register accessors with `const`, bump `pac` MSRV to 1.61
- `fields` fn refactoring
- Test patched STM32
- simplify ci strategy
- Fix generated code for MSP430 atomics

## [v0.27.1] - 2022-10-25

- Fix cli error with --help/version
- Don't cast fields with width 17-31 and non-zero offset.

## [v0.27.0] - 2022-10-24

- Manually inline set/clear_bit
- Don't cast fields with width 17-31
- Make `generic.rs` generic
- [breaking-change] Change initial write value for registers with modifiedWriteValues
- Update `clap` to 4.0, use `irx-config` instead of `clap_conf`
- Add #[must_use] to prevent hanging field writers
- Remove explicit deref in `generic.rs` since it's done by auto-deref
- [breaking-change] Make writing raw bits to a whole register safe if the SVD indicates
  so through the <WriteConstraint> element (see [v0.7.1] too).
- Remove lint #![deny(const_err)] as it is a hard error in Rust now
- Add doc of using `critical-section`

## [v0.26.0] - 2022-10-07

- Use edition 2021
- Fix adding ending reserved field when `max_cluster_size` option enabled
- Add `Eq` autoimplementation for enums
- Use `critical_section::with` instead of `interrupt::free` for `Peripherals::take`.
- Bring documentation on how to generate MSP430 PACs up to date (in line with
  [msp430_svd](https://github.com/pftbest/msp430_svd)).
- Prefix submodule path with self:: when reexporting submodules to avoid ambiguity in crate path.

## [v0.25.1] - 2022-08-22

- Fixed parentheses in RegisterBlock field accessors
- Check cluster size, add `max_cluster_size` option

## [v0.25.0] - 2022-08-02

- Add `feature_peripheral` option which generates cfg features for each peripheral
- Use register aliases in `RegisterBlock` (both structure and mod)
- Create aliases for derived registers & clusters
- Move cluster struct inside mod
- Support non-sequential field arrays
- Use inlined variables in `format!` (Rust 1.58)
- Refactor, clean `periperal.rs` & `util.rs`
- use `svd_parser::expand::Index` for derive
- Generated enum names now consider `name` field in `enumeratedValues`
- Use constant case for structure names; internal rearrangements for
  case conversation traits
- Add new feature `feature_group` which will generate cfg attribute for
  every group name when it is on
- Sort fields by offset before process
- Updated docs for `write` / `modify`

## [v0.24.1] - 2022-07-04

- Make field writer always generic around bit offset (fix bug #620)
- Make binary dependencies optional
- Make JSON and YAML formats optional
- Bump MSRV to 1.60

## [v0.24.0] - 2022-05-12

[commits][v0.24.0]

- Support "nested" `deriveFrom` for registers located in one peripheral
- Use modifiedWriteValues for 1-bitwise fields if present
- Use generic `FieldWriter`, `FieldReader`, `BitWriter`, `BitReader`
- Disable two clippy warnings in `array_proxy.rs`
- Add comments in docs about `readAction`
- Add CI to build and release binaries, use `CHANGELOG.md` as the description
- Optional use `derive_more::{Deref,From}` for register reader & writer
- Don't use prebuilt strategy in CI

## [v0.23.1] - 2022-04-29

- GHA: rust dependency caching
- remove unnedded fields clone
- Use reexport instead of type aliases in `derive_from_base`

## [v0.23.0] - 2022-04-26

- Generate const generic version of field array only if `const_generic` enabled
- Clean `FieldReader`
- Optional PascalCase for Enum values instead of UPPER_CASE
- Add code generation support of peripheral arrays.

## [v0.22.2] - 2022-04-13

- Fix #579 2: support 1-element arrays

## [v0.22.1] - 2022-04-05

- Fix #579

## [v0.22.0] - 2022-04-05

### Added

- added `dyn` keyword to sanatizer.

### Changed

- Generate Rust arrays for all register & cluster arrays with sequential_addresses.
  If their indices don't start from 0 add accessors with right names.
- Bring documentation on how to generate MSP430 PACs up to date (in line with
  [msp430_svd](https://github.com/pftbest/msp430_svd)).
- Use the official SVDs from Espressif for CI and `rust-regress` tests, and
  additionally test the ESP32-C3, ESP32-S2, and ESP32-S3.

## [v0.21.0] - 2022-01-17

### Added

- Support of reading SVD from YAML or JSON files instead of XML

### Changed

- Use `svd-parser` v0.13.1
- Replace suffix in fields' name before converting to snake case when generating methods #563
- MIPS API now re-exports `mips_rt::interrupt` when the `rt` feature is enabled
  but does not generate the `interrupt` macro anymore

### Fixed

- Fix ValidateLevel usage in lib.rs
- Parenthesizing `#offset_calc` to avoid clippy's warning of operator precedence

### Added

- `keep_list` option

## [v0.20.0] - 2021-12-07

### Fixed

- Bug with `use_mask`
- Correct derive for register (cluster) array (needs `svd-rs` 0.11.2)
- New line separators are now rendered in enumerated values
- Multi line field descriptions are now rendered correctly in write and read registers

### Added

- `strict` option
- Missing `inline` on field reader constructors
- Support for device.x generation for riscv targets and `__EXTERNAL_INTERRUPTS` vector table
- Re-export base's module for derived peripherals
- More debug and trace output to visualize program control flow

### Changed

- Use `svd-parser` v0.12
- More Cluster arrays are now emitted as an array rather than a list of
  elements.  An `ArrayProxy` wrapper is used when a Rust built-in array does not
  match the cluster layout.  Requires the `--const_generic` command line option.
- Bumped `xtensa-lx` and add `xtensa_lx::interrupt::InterruptNumber` implementation.
- Don't use a mask when the width of the mask is the same as the width of the parent register.
- Improved error handling
- Registers with single fields that span the entire register now generate safe `bits` writers.

## [v0.19.0] - 2021-05-26

### Added

- MSP430 API for atomically changing register bits, gated behind the `--nightly` flag
- New SVD test for `msp430fr2355`
- Option `-o`(`--output-path`) let you specify output directory path

### Changed

- `\n` in descriptions for multiline
- `_reserved` fields in `RegisterBlock` now hexidemical usize
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

[Unreleased]: https://github.com/rust-embedded/svd2rust/compare/v0.33.4...HEAD
[v0.33.4]: https://github.com/rust-embedded/svd2rust/compare/v0.33.3...v0.33.4
[v0.33.3]: https://github.com/rust-embedded/svd2rust/compare/v0.33.2...v0.33.3
[v0.33.2]: https://github.com/rust-embedded/svd2rust/compare/v0.33.1...v0.33.2
[v0.33.1]: https://github.com/rust-embedded/svd2rust/compare/v0.33.0...v0.33.1
[v0.33.0]: https://github.com/rust-embedded/svd2rust/compare/v0.32.0...v0.33.0
[v0.32.0]: https://github.com/rust-embedded/svd2rust/compare/v0.31.5...v0.32.0
[v0.31.5]: https://github.com/rust-embedded/svd2rust/compare/v0.31.4...v0.31.5
[v0.31.4]: https://github.com/rust-embedded/svd2rust/compare/v0.31.3...v0.31.4
[v0.31.3]: https://github.com/rust-embedded/svd2rust/compare/v0.31.2...v0.31.3
[v0.31.2]: https://github.com/rust-embedded/svd2rust/compare/v0.31.1...v0.31.2
[v0.31.1]: https://github.com/rust-embedded/svd2rust/compare/v0.31.0...v0.31.1
[v0.31.0]: https://github.com/rust-embedded/svd2rust/compare/v0.30.3...v0.31.0
[v0.30.3]: https://github.com/rust-embedded/svd2rust/compare/v0.30.2...v0.30.3
[v0.30.2]: https://github.com/rust-embedded/svd2rust/compare/v0.30.1...v0.30.2
[v0.30.1]: https://github.com/rust-embedded/svd2rust/compare/v0.30.0...v0.30.1
[v0.30.0]: https://github.com/rust-embedded/svd2rust/compare/v0.29.0...v0.30.0
[v0.29.0]: https://github.com/rust-embedded/svd2rust/compare/v0.28.0...v0.29.0
[v0.28.0]: https://github.com/rust-embedded/svd2rust/compare/v0.27.2...v0.28.0
[v0.27.2]: https://github.com/rust-embedded/svd2rust/compare/v0.27.1...v0.27.2
[v0.27.1]: https://github.com/rust-embedded/svd2rust/compare/v0.27.0...v0.27.1
[v0.27.0]: https://github.com/rust-embedded/svd2rust/compare/v0.26.0...v0.27.0
[v0.26.0]: https://github.com/rust-embedded/svd2rust/compare/v0.25.1...v0.26.0
[v0.25.1]: https://github.com/rust-embedded/svd2rust/compare/v0.25.0...v0.25.1
[v0.25.0]: https://github.com/rust-embedded/svd2rust/compare/v0.24.1...v0.25.0
[v0.24.1]: https://github.com/rust-embedded/svd2rust/compare/v0.24.0...v0.24.1
[v0.24.0]: https://github.com/rust-embedded/svd2rust/compare/v0.23.1...v0.24.0
[v0.23.1]: https://github.com/rust-embedded/svd2rust/compare/v0.23.0...v0.23.1
[v0.23.0]: https://github.com/rust-embedded/svd2rust/compare/v0.22.2...v0.23.0
[v0.22.2]: https://github.com/rust-embedded/svd2rust/compare/v0.22.1...v0.22.2
[v0.22.1]: https://github.com/rust-embedded/svd2rust/compare/v0.22.0...v0.22.1
[v0.22.0]: https://github.com/rust-embedded/svd2rust/compare/v0.21.0...v0.22.0
[v0.21.0]: https://github.com/rust-embedded/svd2rust/compare/v0.20.0...v0.21.0
[v0.20.0]: https://github.com/rust-embedded/svd2rust/compare/v0.19.0...v0.20.0
[v0.19.0]: https://github.com/rust-embedded/svd2rust/compare/v0.18.0...v0.19.0
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
