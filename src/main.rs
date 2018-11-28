#![recursion_limit = "128"]

extern crate cast;
extern crate clap;
extern crate either;
#[macro_use]
extern crate error_chain;
extern crate inflections;
#[macro_use]
extern crate quote;
extern crate svd_parser as svd;
extern crate syn;

mod errors;
mod generate;
mod util;

use std::fs::File;
use std::process;
use std::io::{self, Write};

use clap::{App, Arg};

use errors::*;
use util::{build_rs, Target};

fn run() -> Result<()> {
    use std::io::Read;

    let matches = App::new("svd2rust")
        .about("Generate a Rust API from SVD files")
        .arg(
            Arg::with_name("input")
                .help("Input SVD file")
                .short("i")
                .takes_value(true)
                .value_name("FILE"),
        )
        .arg(
            Arg::with_name("target")
                .long("target")
                .help("Target architecture")
                .takes_value(true)
                .value_name("ARCH"),
        )
        .arg(
            Arg::with_name("nightly_features")
                .long("nightly")
                .help("Enable features only available to nightly rustc")
        )
        .version(concat!(
            env!("CARGO_PKG_VERSION"),
            include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
        ))
        .get_matches();

    let target = matches
        .value_of("target")
        .map(|s| Target::parse(s))
        .unwrap_or(Ok(Target::CortexM))?;

    let xml = &mut String::new();
    match matches.value_of("input") {
        Some(file) => {
            File::open(file)
                .chain_err(|| "couldn't open the SVD file")?
                .read_to_string(xml)
                .chain_err(|| "couldn't read the SVD file")?;
        }
        None => {
            let stdin = std::io::stdin();
            stdin
                .lock()
                .read_to_string(xml)
                .chain_err(|| "couldn't read from stdin")?;
        }
    }

    let device = svd::parse(xml);

    let nightly = matches.is_present("nightly_features");

    let mut device_x = String::new();
    let items = generate::device::render(&device, &target, nightly, &mut device_x)?;

    writeln!(File::create("lib.rs").unwrap(), "{}", quote!(#(#items)*)).unwrap();

    if target == Target::CortexM {
        writeln!(File::create("device.x").unwrap(), "{}", device_x).unwrap();
        writeln!(File::create("build.rs").unwrap(), "{}", build_rs()).unwrap();
    }

    Ok(())
}

fn main() {
    use std::io::Write;

    if let Err(ref e) = run() {
        let stderr = io::stderr();
        let mut stderr = stderr.lock();

        writeln!(stderr, "error: {}", e).ok();

        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).ok();
        }

        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace).ok();
        } else {
            writeln!(stderr, "note: run with `RUST_BACKTRACE=1` for a backtrace").ok();
        }

        process::exit(1);
    }
}
