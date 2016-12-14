extern crate clap;
extern crate svd2rust;
extern crate svd_parser as svd;

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
        .version(concat!(env!("CARGO_PKG_VERSION"),
                         include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))))
        .get_matches();

    let xml = &mut String::new();
    File::open(matches.value_of("input").unwrap())
        .unwrap()
        .read_to_string(xml)
        .unwrap();

    let d = svd::parse(xml);
    match matches.value_of("peripheral") {
        None => {
            for peripheral in &d.peripherals {
                println!("const {}: usize = 0x{:08x};",
                         peripheral.name,
                         peripheral.base_address);
            }
        }
        Some(pattern) => {
            if let Some(peripheral) = find_peripheral(&d, |n| n == pattern)
                .or_else(|| find_peripheral(&d, |n| n.contains(pattern))) {
                if let Some(base_peripheral) = peripheral.derived_from.as_ref()
                        .and_then(|bn| find_peripheral(&d, |n| n == bn.to_ascii_lowercase())) {
                    let merged_peripheral = merge(peripheral, base_peripheral);
                    println!("{}", gen_peripheral_desc(&merged_peripheral, &d.defaults));
                } else {
                    println!("{}", gen_peripheral_desc(peripheral, &d.defaults));
                }
            }
        }
    }
}

fn find_peripheral<F: Fn(&str) -> bool>(device: &svd::Device, matcher: F) -> Option<&svd::Peripheral> {
    device.peripherals.iter().find(|x| matcher(&x.name.to_ascii_lowercase()))
}

fn gen_peripheral_desc(p: &svd::Peripheral, def: &svd::Defaults) -> String {
    svd2rust::gen_peripheral(p, &def)
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn merge(p: &svd::Peripheral, bp: &svd::Peripheral) -> svd::Peripheral {
    assert!(p.registers.is_none() || bp.registers.is_none(), "Either {} registers or {} registers must be absent in SVD", p.name, bp.name);

    svd::Peripheral {
        name: p.name.clone(),
        base_address: p.base_address,
        derived_from: None,
        group_name: p.group_name.clone().or_else(|| bp.group_name.clone()),
        description: p.description.clone().or_else(|| bp.description.clone()),
        interrupt: p.interrupt.clone().or_else(|| bp.interrupt.clone()),
        registers: p.registers.clone().or_else(|| bp.registers.clone()),
    }
}
