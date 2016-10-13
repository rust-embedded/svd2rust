#![feature(plugin, rustc_private)]

extern crate clap;
extern crate svd2rust;
extern crate svd_parser as svd;
extern crate syntax;

use std::ascii::AsciiExt;
use std::fs::File;
use std::io::Read;

use clap::{App, Arg};

fn main() {
    let matches = App::new("svd2rust")
        .about("Generate Rust register maps (`struct`s) from SVD files")
        .arg(Arg::with_name("input")
            .help("Input SVD file")
            .required(true)
            .short("i")
            .takes_value(true)
            .value_name("FILE"))
        .arg(Arg::with_name("peripheral")
            .help("Pattern used to select a single peripheral")
            .value_name("PATTERN"))
        .version(include_str!(concat!(env!("OUT_DIR"), "/version.txt")))
        .get_matches();

    let xml = &mut String::new();
    File::open(matches.value_of("input").unwrap()).unwrap().read_to_string(xml).unwrap();

    let d = svd::parse(xml);
    match matches.value_of("peripheral") {
        None => {
            for peripheral in d.peripherals.iter() {
                println!("const {}: usize = 0x{:08x};",
                         peripheral.name,
                         peripheral.base_address);
            }
        }
        Some(pattern) => {
            for peripheral in &d.peripherals {
                if peripheral.name.to_ascii_lowercase().contains(&pattern) {
                    println!("{}",
                             svd2rust::gen_peripheral(peripheral, &d.defaults)
                                 .iter()
                                 .map(|i| i.to_string())
                                 .collect::<Vec<_>>()
                                 .join("\n\n"));

                    break;
                }
            }
        }
    }
}
