#![recursion_limit = "128"]

use std::path::PathBuf;
use svd_parser::svd;
use tracing::{debug, error, info};

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
                Arg::with_name("output")
                    .long("output-dir")
                    .help("Directory to place generated files")
                    .short("o")
                    .takes_value(true)
                    .value_name("PATH"),
            )
            .arg(
                Arg::with_name("config")
                    .long("config")
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
                Arg::with_name("strict")
                    .long("strict")
                    .short("s")
                    .help("Make advanced checks due to parsing SVD"),
            )
            .arg(
                Arg::with_name("log_level")
                    .long("log")
                    .short("l")
                    .help(&format!(
                        "Choose which messages to log (overrides {})",
                        tracing_subscriber::EnvFilter::DEFAULT_ENV
                    ))
                    .takes_value(true),
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

    let path = PathBuf::from(matches.value_of("output").unwrap_or("."));

    let config_filename = matches.value_of("config").unwrap_or("");

    let cfg = with_toml_env(&matches, &[config_filename, "svd2rust.toml"]);

    setup_logging(&cfg);

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
    let strict = cfg.bool_flag("strict", Filter::Arg) || cfg.bool_flag("strict", Filter::Conf);

    let config = Config {
        target,
        nightly,
        generic_mod,
        make_mod,
        const_generic,
        ignore_groups,
        strict,
        output_dir: path.clone(),
    };

    let mut parser_config = svd_parser::Config::default();
    parser_config.validate_level = if strict {
        svd::ValidateLevel::Strict
    } else {
        svd::ValidateLevel::Weak
    };

    info!("Parsing device from SVD file");
    let device = svd_parser::parse_with_config(xml, &parser_config)
        .with_context(|| "Error parsing SVD file".to_string())?;

    let mut device_x = String::new();
    info!("Rendering device");
    let items = generate::device::render(&device, &config, &mut device_x)
        .with_context(|| "Error rendering device")?;

    let filename = if make_mod { "mod.rs" } else { "lib.rs" };
    debug!("Writing files");
    let mut file = File::create(path.join(filename)).expect("Couldn't create output file");

    let data = items.to_string().replace("] ", "]\n");
    file.write_all(data.as_ref())
        .expect("Could not write code to lib.rs");

    if target == Target::CortexM
        || target == Target::Msp430
        || target == Target::XtensaLX
        || target == Target::RISCV
    {
        writeln!(File::create(path.join("device.x"))?, "{}", device_x)?;
        writeln!(File::create(path.join("build.rs"))?, "{}", build_rs())?;
    }
    info!("Code generation completed. Output written to `{}`", path.display());
    Ok(())
}

fn setup_logging<'a>(getter: &'a impl clap_conf::Getter<'a, String>) {
    // * Log at `info` by default.
    // * Allow users the option of setting complex logging filters using
    //   the `RUST_LOG` environment variable.
    // * Override both of those if the logging level is set via
    //   the `log_level` config setting.

    let filter = match getter.grab().arg("log_level").conf("log_level").done() {
        Some(lvl) => tracing_subscriber::EnvFilter::from(lvl),
        None => tracing_subscriber::EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into()),
    };

    tracing_subscriber::fmt()
        .without_time()
        .with_target(false)
        .with_env_filter(filter)
        .compact()
        .with_ansi(true)
        .init();
}

fn main() {
    if let Err(ref e) = run() {
        error!("{:?}", e);

        process::exit(1);
    }
}
