# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/japaric/svd2rust/compare/v0.1.3...HEAD
[v0.1.3]: https://github.com/japaric/svd2rust/compare/v0.1.2...v0.1.3
[v0.1.2]: https://github.com/japaric/svd2rust/compare/v0.1.1...v0.1.2
[v0.1.1]: https://github.com/japaric/svd2rust/compare/v0.1.0...v0.1.1
