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

By default, `svd2rust-regress` assumes you have already built `svd2rust` in the root of this repository in `--release` mode. If this is not possible, it is possible to specify the path to an `svd2rust` binary (see **Options** below).

### Output

For each test case, `svd2rust-regress` will output the result.

Pass results look like this:

```text
Passed: spansion_mb9af12x_k - 23 seconds
```

Fail results look like this:

```text
Failed: si_five_e310x - 0 seconds - Error(Msg("Process Failed! - cargo check"), State { next_error: None, backtrace: None })
```

If all test cases passed, the return code will be `0`. If any test cases failed, the return code will be `1`.

### Options

Here are the options for running `svd2rust-regress`:


```text
svd2rust-regress 0.1.0
James Munns <james.munns@gmail.com>

USAGE:
    svd2rust-regress [FLAGS] [OPTIONS]

FLAGS:
    -b, --bad-tests    Include tests expected to fail (will cause a non-zero return code)
    -h, --help         Prints help information
    -l, --long-test    Run a long test (it's very long)
    -V, --version      Prints version information

OPTIONS:
    -a, --architecture <arch>         Filter by architecture, case sensitive, may be combined with other filters Options
                                      are: "CortexM", "RiscV", and "Msp430"
    -p, --svd2rust-path <bin_path>    Path to an `svd2rust` binary, relative or absolute. Defaults to
                                      `target/release/svd2rust[.exe]` of this repository (which must be already built)
    -c, --chip <chip>                 Filter by chip name, case sensitive, may be combined with other filters
    -m, --manufacturer <mfgr>         Filter by manufacturer, case sensitive, may be combined with other filters
```

### Filters

`svd2rust-regress` allows you to filter which tests will be run. These filters can be combined (but not repeated).

For example, to run all `RiscV` tests:

```bash
# in the ci/svd2rust-regress folder
cargo run --release -- -a RiscV
    Finished release [optimized] target(s) in 0.0 secs
     Running `target/release/svd2rust-regress -a RiscV`
Passed: si_five_e310x - 7 seconds
```

To run against any chip named `MB9AF12xK`:

```bash
cargo run --release -- --long-test -c MB9AF12xK
    Finished release [optimized] target(s) in 0.0 secs
     Running `target/release/svd2rust-regress --long-test -c MB9AF12xK`
Passed: spansion_mb9af12x_k - 23 seconds
Passed: fujitsu_mb9af12x_k - 25 seconds
```

To run against specifically the `Fujitsu` `MB9AF12xK`:
```bash
cargo run --release -- --long-test -c MB9AF12xK -m Fujitsu
    Finished release [optimized] target(s) in 0.0 secs
     Running `target/release/svd2rust-regress --long-test -c MB9AF12xK -m Fujitsu`
Passed: fujitsu_mb9af12x_k - 19 seconds
```