# `svd2rust` Regression Tester

`svd2rust-regress` is a helper program for regression testing changes against `svd2rust`. It uses `rayon` to parallelize testing of multiple chips simultaneously.

## What it does

`svd2rust-regress` will do the following things for each svd/chip tested:

1. Create a new crate for that chip in `output/<chip>`, populated with the architecture specific dependencies
2. Download the `.svd` file for that chip
3. Run `svd2rust` to generate `output/<chip>/src/lib.rs`
4. Run `cargo check` to ensure the project still builds

## Usage

### Preconditions

By default, `svd2rust-regress` assumes you have already built `svd2rust` in the root of this repository in `--release` mode.
If this is not possible, it is possible to specify the path to an `svd2rust` binary (see **Options** below).

You'll also need to have `rustfmt` version > v0.4.0 to use the `--format` flag, you can install `rustfmt` with `rustup component add rustfmt-preview`.

### Output

For each test case, `svd2rust-regress` will output the result.

Pass results look like this:

```text
Passed: spansion_mb9af12x_k - 23 seconds
```

Fail results look like this:

```text
Failed: si_five_e310x - 0 seconds. Error: Process Failed - cargo check
```

If all test cases passed, the return code will be `0`. If any test cases failed, the return code will be `1`.

### Options

You can display options for `svd2rust-regress` by running:

```text
# in the ci/svd2rust-regress folder
cargo regress help
```

### Filters

`svd2rust-regress` allows you to filter which tests will be run. These filters can be combined (but not repeated).

For example, to run all `RiscV` tests:

```bash
# in the ci/svd2rust-regress folder
cargo regress tests --architecture riscv
```

To run against any chip named `MB9AF12xK`:

```bash
cargo regress test -c MB9AF12xK
```

To run against specifically the `Fujitsu` `MB9AF12xK`:
```bash
cargo regress test -c MB9AF12xK -m Fujitsu
```
