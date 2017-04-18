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
extern crate rustfmt;

mod errors;
mod generate;
mod util;

use std::fs::File;
use std::{io, process};

use clap::{App, Arg};

use errors::*;

fn run() -> Result<()> {
    use std::io::Read;

    let matches = App::new("svd2rust")
        .about("Generate a Rust API from SVD files")
        .arg(Arg::with_name("rustfmt")
                 .short("r")
                 .long("rustfmt")
                 .help("Pretty-print the output with rustfmt"))
        .arg(Arg::with_name("input")
                 .help("Input SVD file")
                 .required(true)
                 .short("i")
                 .takes_value(true)
                 .value_name("FILE"))
        .version(concat!(env!("CARGO_PKG_VERSION"),
                         include_str!(concat!(env!("OUT_DIR"),
                                              "/commit-info.txt"))))
        .get_matches();

    let xml = &mut String::new();
    File::open(matches.value_of("input").unwrap())
        .chain_err(|| "couldn't open the SVD file")?
        .read_to_string(xml)
        .chain_err(|| "couldn't read the SVD file")?;

    let device = svd::parse(xml);

    let mut items = vec![];
    generate::device(&device, &mut items)?;

    if matches.is_present("rustfmt") {
        let text = format!("{}",
                           quote! {
                               #(#items)*
                           });
        
        use rustfmt::config;
        let config: config::Config =
            config::Config { error_on_line_overflow: false,
                             write_mode: config::WriteMode::Plain,
                             ..Default::default() };
        let result = rustfmt::run(rustfmt::Input::Text(text), &config);

        if result.has_no_errors() {
            Ok(())
        } else {
            println!("Rustfmt error: {:?}", result);
            Err("rustfmt error!".into())
        }
    } else {
        println!("{}",
                 quote! {
                     #(#items)*
                 });
        Ok(())
    }

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
            writeln!(stderr,
                     "note: run with `RUST_BACKTRACE=1` for a backtrace")
                    .ok();
        }

        process::exit(1);
    }
}
