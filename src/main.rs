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
extern crate toml;

mod errors;
mod generate;
mod util;

use std::fs::File;
use std::io::{self, Write};
use std::process;

use clap::{App, Arg};
use quote::Tokens;

use errors::*;
use generate::device::RenderOutput;

#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    CortexM,
    Msp430,
    RISCV,
    None,
}

impl Target {
    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "cortex-m" => Target::CortexM,
            "msp430" => Target::Msp430,
            "riscv" => Target::RISCV,
            "none" => Target::None,
            _ => bail!("unknown target {}", s),
        })
    }
}

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
        ).arg(
            Arg::with_name("target")
                .long("target")
                .help("Target architecture")
                .takes_value(true)
                .value_name("ARCH"),
        ).arg(
            Arg::with_name("nightly_features")
                .long("nightly")
                .help("Enable features only available to nightly rustc"),
        ).arg(
            Arg::with_name("conditional_peripherals")
                .long("cond-periphs")
                .short("p")
                .help("Wrap each generated peripheral in a conditional feature"),
        ).version(concat!(
            env!("CARGO_PKG_VERSION"),
            include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
        )).get_matches();

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
    let conditional = matches.is_present("conditional_peripherals");

    let mut device_x = String::new();

    #[allow(unused)] // Required because `features` is used conditionally
    let RenderOutput { tokens, features } =
        generate::device::render(&device, &target, nightly, conditional, &mut device_x)?;

    if target == Target::CortexM {
        writeln!(File::create("lib.rs").unwrap(), "{}", quote!(#(#tokens)*)).unwrap();
        writeln!(File::create("device.x").unwrap(), "{}", device_x).unwrap();
        writeln!(File::create("build.rs").unwrap(), "{}", build_rs()).unwrap();
    } else {
        println!("{}", quote!(#(#tokens)*));
    }

    // Only generate `Cargo.toml` when feature was selected
    if conditional {
        writeln!(
            File::create("CargoFeatures.toml").unwrap(),
            "{}",
            generate::cargo::generate_skeleton(features)
        ).unwrap();
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

fn build_rs() -> Tokens {
    quote! {
        use std::env;
        use std::fs::File;
        use std::io::Write;
        use std::path::PathBuf;

        fn main() {
            if env::var_os("CARGO_FEATURE_RT").is_some() {
                // Put the linker script somewhere the linker can find it
                let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
                File::create(out.join("device.x"))
                    .unwrap()
                    .write_all(include_bytes!("device.x"))
                    .unwrap();
                println!("cargo:rustc-link-search={}", out.display());

                println!("cargo:rerun-if-changed=device.x");
            }

            println!("cargo:rerun-if-changed=build.rs");
        }
    }
}
