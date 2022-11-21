[![crates.io](https://img.shields.io/crates/d/svd2rust.svg)](https://crates.io/crates/svd2rust)
[![crates.io](https://img.shields.io/crates/v/svd2rust.svg)](https://crates.io/crates/svd2rust)

# `svd2rust`

> Generate Rust register maps (`struct`s) from SVD files

This project is developed and maintained by the [Tools team][team].

# [Documentation](https://docs.rs/svd2rust)

# [API](https://docs.rs/svd2rust)

## Minimum Supported Rust Version (MSRV)

The **generated code** is guaranteed to compile on stable Rust 1.61.0 and up.

If you encounter compilation errors on any stable version newer than 1.61.0, please open an issue.

# Testing Locally

`svd2rust-regress` is a helper program for regression testing changes against `svd2rust`. This tool can be used locally to check modifications of `svd2rust` locally before submitting a PR.

Check out the [svd2rust-regress README](ci/svd2rust-regress/README.md) for information on how to use this tool.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

## Code of Conduct

Contribution to this crate is organized under the terms of the [Rust Code of
Conduct][CoC], the maintainer of this crate, the [Tools team][team], promises
to intervene to uphold that code of conduct.

[CoC]: CODE_OF_CONDUCT.md
[team]: https://github.com/rust-embedded/wg#the-tools-team
