#![recursion_limit = "128"]

use log::error;
use svd_parser as svd;

mod generate;
mod util;

use std::fs::File;
use std::io::Write;
use std::process;

use anyhow::{Context, Result};
use clap::{App, Arg};

use crate::util::{build_rs, Config, Target};

fn run() -> Result<()> {
    use clap_conf::prelude::*;
    use std::io::Read;

    let matches =
        App::new("svd2rust")
            .about("Generate a Rust API from SVD files")
            .arg(
                Arg::with_name("input")
                    .help("Input SVD file")
                    .short("i")
                    .takes_value(true)
                    .value_name("FILE"),
            )
            .arg(
                Arg::with_name("config")
                    .help("Config TOML file")
                    .short("c")
                    .takes_value(true)
                    .value_name("TOML_FILE"),
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
            .arg(Arg::with_name("const_generic").long("const_generic").help(
                "Use const generics to generate writers for same fields with different offsets",
            ))
            .arg(
                Arg::with_name("ignore_groups")
                    .long("ignore_groups")
                    .help("Don't add alternateGroup name as prefix to register name"),
            )
            .arg(
                Arg::with_name("generic_mod")
                    .long("generic_mod")
                    .short("g")
                    .help("Push generic mod in separate file"),
            )
            .arg(
                Arg::with_name("make_mod")
                    .long("make_mod")
                    .short("m")
                    .help("Create mod.rs instead of lib.rs, without inner attributes"),
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
                    .possible_values(&["off", "error", "warn", "info", "debug", "trace"]),
            )
            .version(concat!(
                env!("CARGO_PKG_VERSION"),
                include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
            ))
            .get_matches();

    let xml = &mut String::new();
    match matches.value_of("input") {
        Some(file) => {
            File::open(file)
                .context("Cannot open the SVD file")?
                .read_to_string(xml)
                .context("Cannot read the SVD file")?;
        }
        None => {
            let stdin = std::io::stdin();
            stdin
                .lock()
                .read_to_string(xml)
                .context("Cannot read from stdin")?;
        }
    }

    let config_filename = matches.value_of("config").unwrap_or("");

    let cfg = with_toml_env(&matches, &[config_filename, "svd2rust.toml"]);

    setup_logging(&cfg);

    let device = svd::parse(xml)?;

    let target = cfg
        .grab()
        .arg("target")
        .conf("target")
        .done()
        .map(|s| Target::parse(&s))
        .unwrap_or_else(|| Ok(Target::default()))?;

    let nightly =
        cfg.bool_flag("nightly_features", Filter::Arg) || cfg.bool_flag("nightly", Filter::Conf);
    let generic_mod =
        cfg.bool_flag("generic_mod", Filter::Arg) || cfg.bool_flag("generic_mod", Filter::Conf);
    let make_mod =
        cfg.bool_flag("make_mod", Filter::Arg) || cfg.bool_flag("make_mod", Filter::Conf);
    let const_generic =
        cfg.bool_flag("const_generic", Filter::Arg) || cfg.bool_flag("const_generic", Filter::Conf);
    let ignore_groups =
        cfg.bool_flag("ignore_groups", Filter::Arg) || cfg.bool_flag("ignore_groups", Filter::Conf);

    let config = Config {
        target,
        nightly,
        generic_mod,
        make_mod,
        const_generic,
        ignore_groups,
    };

    let mut device_x = String::new();
    let items = generate::device::render(&device, &config, &mut device_x)?;
    let filename = if make_mod { "mod.rs" } else { "lib.rs" };
    let mut file = File::create(filename).expect("Couldn't create output file");

    let data = items.to_string().replace("] ", "]\n");
    file.write_all(data.as_ref())
        .expect("Could not write code to lib.rs");

    if target == Target::CortexM || target == Target::Msp430 || target == Target::XtensaLX {
        writeln!(File::create("device.x")?, "{}", device_x)?;
        writeln!(File::create("build.rs")?, "{}", build_rs())?;
    }

    Ok(())
}

fn setup_logging<'a>(getter: &'a impl clap_conf::Getter<'a, String>) {
    // * Log at info by default.
    // * Allow users the option of setting complex logging filters using
    //   env_logger's `RUST_LOG` environment variable.
    // * Override both of those if the logging level is set via the `--log`
    //   command line argument.
    let env = env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
    let mut builder = env_logger::Builder::from_env(env);
    builder.format_timestamp(None);

    let log_lvl_from_env = std::env::var_os(env_logger::DEFAULT_FILTER_ENV).is_some();

    if log_lvl_from_env {
        log::set_max_level(log::LevelFilter::Trace);
    } else {
        let level = match getter.grab().arg("log_level").conf("log_level").done() {
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
        error!("{:?}", e);

        process::exit(1);
    }
}
