#![recursion_limit = "128"]

use log::{error, info};
use std::path::{Path, PathBuf};

use std::fs::File;
use std::io::Write;
use std::process;

use anyhow::{Context, Result};
use clap::{App, Arg};

use svd2rust::{
    generate, load_from,
    util::{build_rs, group_names, Config, SourceType, Target},
};

fn run() -> Result<()> {
    use clap_conf::prelude::*;
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
        .arg(
            Arg::with_name("ignore_groups")
                .long("ignore_groups")
                .help("Don't add alternateGroup name as prefix to register name"),
        )
        .arg(Arg::with_name("keep_list").long("keep_list").help(
            "Keep lists when generating code of dimElement, instead of trying to generate arrays",
        ))
        .arg(
            Arg::with_name("generic_mod")
                .long("generic_mod")
                .short("g")
                .help("Push generic mod in separate file"),
        )
        .arg(
            Arg::with_name("feature_group")
                .long("feature_group")
                .help("Use group_name of peripherals as feature"),
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
            Arg::with_name("pascal_enum_values")
                .long("pascal_enum_values")
                .help("Use PascalCase in stead of UPPER_CASE for enumerated values"),
        )
        .arg(
            Arg::with_name("derive_more")
                .long("derive_more")
                .help("Use derive_more procedural macros to implement Deref and From"),
        )
        .arg(
            Arg::with_name("source_type")
                .long("source_type")
                .help("Specify file/stream format"),
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

    let input = &mut String::new();
    match matches.value_of("input") {
        Some(file) => {
            File::open(file)
                .context("Cannot open the SVD file")?
                .read_to_string(input)
                .context("Cannot read the SVD file")?;
        }
        None => {
            let stdin = std::io::stdin();
            stdin
                .lock()
                .read_to_string(input)
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
    let ignore_groups =
        cfg.bool_flag("ignore_groups", Filter::Arg) || cfg.bool_flag("ignore_groups", Filter::Conf);
    let keep_list =
        cfg.bool_flag("keep_list", Filter::Arg) || cfg.bool_flag("keep_list", Filter::Conf);
    let strict = cfg.bool_flag("strict", Filter::Arg) || cfg.bool_flag("strict", Filter::Conf);
    let pascal_enum_values = cfg.bool_flag("pascal_enum_values", Filter::Arg)
        || cfg.bool_flag("pascal_enum_values", Filter::Conf);
    let derive_more =
        cfg.bool_flag("derive_more", Filter::Arg) || cfg.bool_flag("derive_more", Filter::Conf);
    let feature_group =
        cfg.bool_flag("feature_group", Filter::Arg) || cfg.bool_flag("feature_group", Filter::Conf);

    let mut source_type = cfg
        .grab()
        .arg("source_type")
        .conf("source_type")
        .done()
        .and_then(|s| SourceType::from_extension(&s))
        .unwrap_or_default();

    if let Some(file) = matches.value_of("input") {
        source_type = SourceType::from_path(Path::new(file))
    }

    let config = Config {
        target,
        nightly,
        generic_mod,
        make_mod,
        ignore_groups,
        keep_list,
        strict,
        pascal_enum_values,
        derive_more,
        feature_group,
        output_dir: path.clone(),
        source_type,
    };

    info!("Parsing device from SVD file");
    let device = load_from(input, &config)?;

    let mut device_x = String::new();
    info!("Rendering device");
    let items = generate::device::render(&device, &config, &mut device_x)
        .with_context(|| "Error rendering device")?;

    let filename = if make_mod { "mod.rs" } else { "lib.rs" };
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

    if feature_group {
        let group_names: Vec<String> = group_names(&device)
            .iter()
            .map(|s| format!("{} = []\n", s))
            .collect();
        write!(
            File::create(path.join("features.toml"))?,
            "# Below are the FEATURES generated by svd2rust base on groupName in SVD file.\n\
            # Please copy them to Cargo.toml.\n\
            [features]\n\
            {}",
            group_names.join("")
        )?;
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
