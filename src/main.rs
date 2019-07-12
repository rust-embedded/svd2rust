#![recursion_limit = "128"]

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quote;
use svd_parser as svd;

mod errors;
mod generate;
mod util;

use std::fs::File;
use std::io::{self, Write};
use std::process;

use clap::{App, Arg};

use crate::errors::*;
use crate::util::{build_rs, Target};

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
                .help("Enable features only available to nightly rustc"),
        )
        .arg(
            Arg::with_name("log_level")
                .long("log")
                .short("l")
                .help(&format!(
                    "Choose which messages to log (overrides {})",
                    env_logger::DEFAULT_FILTER_ENV
                ))
                .takes_value(true)
                .possible_values(&["off", "error", "warn", "info", "debug", "trace"])
        )
        .version(concat!(
            env!("CARGO_PKG_VERSION"),
            include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
        ))
        .get_matches();

    setup_logging(&matches);

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

    let device = svd::parse(xml).unwrap(); //TODO(AJM)

    let nightly = matches.is_present("nightly_features");

    let mut device_x = String::new();
    let items = generate::device::render(&device, target, nightly, &mut device_x)?;

    writeln!(File::create("lib.rs").unwrap(), "{}", quote!(#(#items)*)).unwrap();

    if target == Target::CortexM {
        writeln!(File::create("device.x").unwrap(), "{}", device_x).unwrap();
        writeln!(File::create("build.rs").unwrap(), "{}", build_rs()).unwrap();
    }

    Ok(())
}

fn setup_logging(matches: &clap::ArgMatches) {
    // * Log at info by default.
    // * Allow users the option of setting complex logging filters using
    //   env_logger's `RUST_LOG` environment variable.
    // * Override both of those if the logging level is set via the `--log`
    //   command line argument.
    let env = env_logger::Env::default()
        .filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
    let mut builder = env_logger::Builder::from_env(env);
    builder.default_format_timestamp(false);

    let log_lvl_from_env =
        std::env::var_os(env_logger::DEFAULT_FILTER_ENV).is_some();

    if log_lvl_from_env {
        log::set_max_level(log::LevelFilter::Trace);
    } else {
        let level = match matches.value_of("log_level") {
            Some(lvl) => lvl.parse().unwrap(),
            None => log::LevelFilter::Info,
        };
        log::set_max_level(level);
        builder.filter_level(level);
    }

    builder.init();
}

fn main() {
    if let Err(ref e) = run() {
        error!("{}", e);

        for e in e.iter().skip(1) {
            error!("caused by: {}", e);
        }

        if let Some(backtrace) = e.backtrace() {
            error!("backtrace: {:?}", backtrace);
        } else {
            error!("note: run with `RUST_BACKTRACE=1` for a backtrace")
        }

        process::exit(1);
    }
}
