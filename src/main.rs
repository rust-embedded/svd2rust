#![recursion_limit = "128"]

use log::{debug, error, info, warn};
use svd2rust::config::{IdentFormatError, IdentFormats, IdentFormatsTheme};
use svd2rust::util::IdentFormat;

use std::io::Write;
use std::process;
use std::{fs::File, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command};

use svd2rust::{
    config::{Config, SourceType, Target},
    generate, load_from,
    util::{self, build_rs},
};

fn parse_configs(app: Command) -> Result<Config> {
    use irx_config::parsers::{cmd, toml};
    use irx_config::ConfigBuilder;
    let ident_formats = app.clone().get_matches();
    let irxconfig = ConfigBuilder::default()
        .append_parser(cmd::ParserBuilder::new(app).exit_on_error(true).build()?)
        .append_parser(
            toml::ParserBuilder::default()
                .default_path("svd2rust.toml")
                .path_option("config")
                .ignore_missing_file(true)
                .build()?,
        )
        .load()?;

    let mut config: Config = irxconfig.get()?;
    let mut idf = match config.ident_formats_theme {
        IdentFormatsTheme::New => IdentFormats::new_theme(),
        IdentFormatsTheme::Legacy => IdentFormats::legacy_theme(),
    };
    idf.extend(config.ident_formats.drain());
    config.ident_formats = idf;

    if let Some(ident_formats) = ident_formats.get_many::<String>("ident_format") {
        for fs in ident_formats {
            if let Some((n, fmt)) = fs.split_once(':') {
                if let std::collections::hash_map::Entry::Occupied(mut e) =
                    config.ident_formats.entry(n.into())
                {
                    match IdentFormat::parse(fmt) {
                        Ok(ident_format) => {
                            e.insert(ident_format);
                        }
                        Err(IdentFormatError::UnknownCase(c)) => {
                            warn!("Ident case `{c}` is unknown")
                        }
                        Err(IdentFormatError::Other) => {
                            warn!("Can't parse identifier format string `{fmt}`")
                        }
                    }
                } else {
                    warn!("Ident format name `{n}` is unknown");
                }
            } else {
                warn!("Can't parse identifier format string `{fs}`");
            }
        }
    }

    Ok(config)
}

fn run() -> Result<()> {
    use std::io::Read;

    let app = Command::new("svd2rust")
        .about("Generate a Rust API from SVD files")
        .arg(
            Arg::new("input")
                .help("Input SVD file")
                .short('i')
                .action(ArgAction::Set)
                .value_name("FILE"),
        )
        .arg(
            Arg::new("output_dir")
                .long("output-dir")
                .alias("output_dir")
                .help("Directory to place generated files")
                .short('o')
                .action(ArgAction::Set)
                .value_name("PATH"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .help("Config TOML file")
                .short('c')
                .action(ArgAction::Set)
                .value_name("TOML_FILE"),
        )
        .arg(
            Arg::new("target")
                .long("target")
                .help("Target architecture")
                .action(ArgAction::Set)
                .value_name("ARCH"),
        )
        .arg(
            Arg::new("atomics")
                .long("atomics")
                .action(ArgAction::SetTrue)
                .help("Generate atomic register modification API"),
        )
        .arg(
            Arg::new("atomics_feature")
                .long("atomics-feature")
                .alias("atomics_feature")
                .help("add feature gating for atomic register modification API")
                .action(ArgAction::Set)
                .value_name("FEATURE"),
        )
        .arg(
            Arg::new("ignore_groups")
                .long("ignore-groups")
                .alias("ignore_groups")
                .action(ArgAction::SetTrue)
                .help("Don't add alternateGroup name as prefix to register name"),
        )
        .arg(
            Arg::new("keep_list")
            .long("keep-list")
            .alias("keep_list")
            .action(ArgAction::SetTrue)
            .help(
            "Keep lists when generating code of dimElement, instead of trying to generate arrays",
        ))
        .arg(
            Arg::new("generic_mod")
                .long("generic-mod")
                .alias("generic_mod")
                .short('g')
                .action(ArgAction::SetTrue)
                .help("Push generic mod in separate file"),
        )
        .arg(
            Arg::new("feature_group")
                .long("feature-group")
                .alias("feature_group")
                .action(ArgAction::SetTrue)
                .help("Use group_name of peripherals as feature"),
        )
        .arg(
            Arg::new("feature_peripheral")
                .long("feature-peripheral")
                .alias("feature_peripheral")
                .action(ArgAction::SetTrue)
                .help("Use independent cfg feature flags for each peripheral"),
        )
        .arg(
            Arg::new("ident_format")
                .long("ident-format")
                .short('f')
                .alias("ident_format")
                .action(ArgAction::Append)
                .long_help(
format!("Specify `-f type:prefix:case:suffix` to change default ident formatting.
Allowed values of `type` are {:?}.
Allowed cases are `unchanged` (''), `pascal` ('p'), `constant` ('c') and `snake` ('s').
", IdentFormats::new_theme().keys().collect::<Vec<_>>())
),
        )
        .arg(
            Arg::new("ident_formats_theme")
                .long("ident-formats-theme")
                .help("A set of `ident_format` settings. `new` or `legacy`")
                .action(ArgAction::Set)
                .value_name("THEME"),
        )
        .arg(
            Arg::new("max_cluster_size")
                .long("max-cluster-size")
                .alias("max_cluster_size")
                .action(ArgAction::SetTrue)
                .help("Use array increment for cluster size"),
        )
        .arg(
            Arg::new("impl_debug")
                .long("impl-debug")
                .alias("impl_debug")
                .action(ArgAction::SetTrue)
                .help("implement Debug for readable blocks and registers"),
        )
        .arg(
            Arg::new("impl_debug_feature")
                .long("impl-debug-feature")
                .alias("impl_debug_feature")
                .help("Add feature gating for block and register debug implementation")
                .action(ArgAction::Set)
                .value_name("FEATURE"),
        )
        .arg(
            Arg::new("impl_defmt")
                .long("impl-defmt")
                .alias("impl_defmt")
                .help("Add automatic defmt implementation for enumerated values")
                .action(ArgAction::Set)
                .value_name("FEATURE"),
        )
        .arg(
            Arg::new("make_mod")
                .long("make-mod")
                .alias("make_mod")
                .short('m')
                .action(ArgAction::SetTrue)
                .help("Create mod.rs instead of lib.rs, without inner attributes"),
        )
        .arg(
            Arg::new("skip_crate_attributes")
                .long("skip-crate-attributes")
                .alias("skip_crate_attributes")
                .action(ArgAction::SetTrue)
                .help("Do not generate crate attributes"),
        )
        .arg(
            Arg::new("strict")
                .long("strict")
                .short('s')
                .action(ArgAction::SetTrue)
                .help("Make advanced checks due to parsing SVD"),
        )
        .arg(
            Arg::new("source_type")
                .long("source-type")
                .alias("source_type")
                .help("Specify file/stream format"),
        )
        .arg(
            Arg::new("reexport_core_peripherals")
                .long("reexport-core-peripherals")
                .alias("reexport_core_peripherals")
                .action(ArgAction::SetTrue)
                .help("For Cortex-M target reexport peripherals from cortex-m crate"),
        )
        .arg(
            Arg::new("reexport_interrupt")
                .long("reexport-interrupt")
                .alias("reexport_interrupt")
                .action(ArgAction::SetTrue)
                .help("Reexport interrupt macro from cortex-m-rt like crates"),
        )
        .arg(
            Arg::new("base_address_shift")
                .short('b')
                .long("base-address-shift")
                .alias("base_address_shift")
                .action(ArgAction::Set)
                .help("Add offset to all base addresses on all peripherals in the SVD file.")
                .long_help("Add offset to all base addresses on all peripherals in the SVD file.
Useful for soft-cores where the peripheral address range isn't necessarily fixed.
Ignore this option if you are not building your own FPGA based soft-cores."),
        )
        .arg(
            Arg::new("log_level")
                .long("log")
                .short('l')
                .help(format!(
                    "Choose which messages to log (overrides {})",
                    env_logger::DEFAULT_FILTER_ENV
                ))
                .action(ArgAction::Set)
                .value_parser(["off", "error", "warn", "info", "debug", "trace"]),
        )
        .version(concat!(
            env!("CARGO_PKG_VERSION"),
            include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
        ));

    let mut config = match parse_configs(app) {
        Ok(config) => {
            setup_logging(&config.log_level);
            config
        }
        Err(e) => {
            setup_logging(&None);
            return Err(e);
        }
    };

    debug!("Current svd2rust config: {config:#?}");

    let input = &mut String::new();
    match config.input.as_ref() {
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

    if let Some(file) = config.input.as_ref() {
        config.source_type = SourceType::from_path(file)
    }
    let path = config.output_dir.as_deref().unwrap_or(Path::new("."));

    info!("Parsing device from SVD file");
    let device = load_from(input, &config)?;

    let mut device_x = String::new();
    info!("Rendering device");
    let items = generate::device::render(&device, &config, &mut device_x)
        .with_context(|| "Error rendering device")?;

    let filename = if config.make_mod { "mod.rs" } else { "lib.rs" };
    let mut file = File::create(path.join(filename)).expect("Couldn't create output file");

    let data = items.to_string().replace("] ", "]\n");
    file.write_all(data.as_ref())
        .expect("Could not write code to lib.rs");

    if [
        Target::CortexM,
        Target::Msp430,
        Target::XtensaLX,
        Target::RISCV,
    ]
    .contains(&config.target)
    {
        writeln!(File::create(path.join("device.x"))?, "{device_x}")?;
        writeln!(File::create(path.join("build.rs"))?, "{}", build_rs())?;
    }

    if config.feature_group || config.feature_peripheral {
        let feature_format = config.ident_formats.get("peripheral_feature").unwrap();
        let mut features = Vec::new();
        if config.feature_group {
            features.extend(
                util::group_names(&device, &feature_format)
                    .iter()
                    .map(|s| format!("{s} = []\n")),
            );
            let add_groups: Vec<_> = util::group_names(&device, &feature_format)
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect();
            features.push(format!("all-groups = [{}]\n", add_groups.join(",")))
        }
        if config.feature_peripheral {
            features.extend(
                util::peripheral_names(&device, &feature_format)
                    .iter()
                    .map(|s| format!("{s} = []\n")),
            );
            let add_peripherals: Vec<_> = util::peripheral_names(&device, &feature_format)
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect();
            features.push(format!(
                "all-peripherals = [{}]\n",
                add_peripherals.join(",")
            ))
        }
        write!(
            File::create(path.join("features.toml"))?,
            "# Below are the FEATURES generated by svd2rust base on groupName in SVD file.\n\
            # Please copy them to Cargo.toml.\n\
            [features]\n\
            {}",
            features.join("")
        )?;
    }

    Ok(())
}

fn setup_logging(log_level: &Option<String>) {
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
        let level = match log_level {
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
        error!("{e:?}");

        process::exit(1);
    }
}
