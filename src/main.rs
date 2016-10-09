#![feature(plugin, rustc_private)]
#![plugin(quasi_macros)]

extern crate clap;
extern crate svd2rust;
extern crate svd;
extern crate syntax;

use std::ascii::AsciiExt;
use std::fs::File;
use std::io::Read;
use syntax::ext::base::DummyResolver;
use syntax::parse::ParseSess;
use syntax::print::pprust;

use svd::Device;
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

    let sess = &ParseSess::new();
    let macro_loader = &mut DummyResolver;
    let cx = &svd2rust::make_ext_ctxt(sess, macro_loader);

    let xml = &mut String::new();
    File::open(matches.value_of("input").unwrap()).unwrap().read_to_string(xml).unwrap();

    let d = Device::parse(xml);
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
                             svd2rust::gen_peripheral(cx, peripheral, &d.defaults)
                                 .iter()
                                 .map(|i| pprust::item_to_string(i))
                                 .collect::<Vec<_>>()
                                 .join("\n\n"));

                    break;
                }
            }
        }
    }
}
