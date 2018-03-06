#[macro_use]
extern crate error_chain;
extern crate inflections;
extern crate rayon;
extern crate reqwest;
#[macro_use]
extern crate structopt;

mod tests;
mod errors;
mod svd_test;

use std::path::PathBuf;
use structopt::StructOpt;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::exit;
use std::time::Instant;

#[derive(StructOpt, Debug)]
#[structopt(name = "svd2rust-regress")]
struct Opt {
    /// Run a long test (it's very long)
    #[structopt(short = "l", long = "long-test")]
    long_test: bool,

    /// Path to an `svd2rust` binary, relative or absolute.
    /// Defaults to `target/release/svd2rust[.exe]` of this repository
    /// (which must be already built)
    #[structopt(short = "p", long = "svd2rust-path", parse(from_os_str))]
    bin_path: Option<PathBuf>,

    /// Filter by chip name, case sensitive, may be combined with other filters
    #[structopt(short = "c", long = "chip")]
    chip: Option<String>,

    /// Filter by manufacturer, case sensitive, may be combined with other filters
    #[structopt(short = "m", long = "manufacturer")]
    mfgr: Option<String>,

    /// Filter by architecture, case sensitive, may be combined with other filters
    /// Options are: "CortexM", "RiscV", and "Msp430"
    #[structopt(short = "a", long = "architecture")]
    arch: Option<String>,

    /// Include tests expected to fail (will cause a non-zero return code)
    #[structopt(short = "b", long = "bad-tests")]
    bad_tests: bool,

    // TODO: Specify smaller subset of tests? Maybe with tags?
    // TODO: Early fail
    // TODO: Compile svd2rust?
}

/// Validate any assumptions made by this program
fn validate_tests(tests: &[&tests::TestCase]) {
    use std::collections::HashSet;

    let mut fail = false;

    // CONDITION 1: All mfgr+chip names must be unique
    let mut uniq = HashSet::new();
    for t in tests {
        let name = t.name();
        if !uniq.insert(name.clone()) {
            fail = true;
            eprintln!("{} is not unique!", name);
        }
    }

    if fail {
        panic!("Tests failed validation");
    }
}

fn main() {
    let opt = Opt::from_args();

    // Validate all test pre-conditions
    validate_tests(tests::TESTS);

    // Determine default svd2rust path
    let default_svd2rust_iter = ["..", "..", "..", "..", "target", "release"];

    let default_svd2rust = if cfg!(windows) {
        default_svd2rust_iter.iter().chain(["svd2rust.exe"].iter())
    } else {
        default_svd2rust_iter.iter().chain(["svd2rust"].iter())
    }.collect();

    let bin_path = match opt.bin_path {
        Some(ref bp) => bp,
        None => &default_svd2rust,
    };

    // collect enabled tests
    let tests = tests::TESTS
        .iter()
        // Short test?
        .filter(|t| t.should_run(!opt.long_test))
        // selected architecture?
        .filter(|t| {
            if let Some(ref arch) = opt.arch {
                arch == &format!("{:?}", t.arch)
            } else {
                true
            }
        })
        // selected manufacturer?
        .filter(|t| {
            if let Some(ref mfgr) = opt.mfgr {
                mfgr == &format!("{:?}", t.mfgr)
            } else {
                true
            }
        })
        // Specify chip - note: may match multiple
        .filter(|t| {
            if let Some(ref chip) = opt.chip {
                chip == t.chip
            } else {
                true
            }
        })
        // Run failable tests?
        .filter(|t| opt.bad_tests || t.should_pass)
        .collect::<Vec<_>>();

    let any_fails = AtomicBool::new(false);

    // TODO: It would be more efficient to reuse directories, so we don't
    // have to rebuild all the deps crates
    tests.par_iter().for_each(|t| {
        let start = Instant::now();

        match svd_test::test(t, &bin_path) {
            Ok(()) => {
                eprintln!(
                    "Passed: {} - {} seconds",
                    t.name(),
                    start.elapsed().as_secs()
                );
            }
            Err(e) => {
                // TODO: I think this is the right ordering. I don't think we
                // care about any reads until we are done with the parallel part,
                // though performance probably doesn't matter because each iter
                // takes ~minutes
                any_fails.store(true, Ordering::Release);
                eprintln!(
                    "Failed: {} - {} seconds - {:?}",
                    t.name(),
                    start.elapsed().as_secs(),
                    e
                );
            }
        }
    });

    if any_fails.load(Ordering::Acquire) {
        exit(1);
    } else {
        exit(0);
    }
}
