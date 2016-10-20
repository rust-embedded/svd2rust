# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

### Fixed

- Some SVD files specify that two registers exist at the same address.
  `svd2rust` didn't handle this case and panicked. A proper solution to handle
  this case will require `union`s but those have not been stabilized. For now,
  `svd2rust` will simply pick one of the two or more registers that overlap and
  ignore the rest.

## v0.1.0 - 2016-10-15

### Added

- Initial version of the `svd2rust` tool

[Unreleased]: https://github.com/japaric/svd2rust/compare/v0.1.0...HEAD
