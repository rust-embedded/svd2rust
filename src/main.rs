#![recursion_limit = "128"]

use log::{debug, error, info};

use std::fs::File;
use std::io::Write;
use std::process;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command};

use svd2rust::{
    generate, load_from,
    util::{self, build_rs, Config, SourceType, Target},
};

fn parse_configs(app: Command) -> Result<Config> {
    use irx_config::parsers::{cmd, toml};
    use irx_config::ConfigBuilder;
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

    irxconfig.get().map_err(Into::into)
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
                .long("atomics_feature")
                .help("add feature gating for atomic register modification API")
                .action(ArgAction::Set)
                .value_name("FEATURE"),
        )
        .arg(
            Arg::new("array_proxy")
            .long("array_proxy")
            .action(ArgAction::SetTrue)
            .help(
                "Use ArrayProxy helper for non-sequential register arrays",
            ),
        )
        .arg(
            Arg::new("ignore_groups")
                .long("ignore_groups")
                .action(ArgAction::SetTrue)
                .help("Don't add alternateGroup name as prefix to register name"),
        )
        .arg(
            Arg::new("keep_list")
            .long("keep_list")
            .action(ArgAction::SetTrue)
            .help(
            "Keep lists when generating code of dimElement, instead of trying to generate arrays",
        ))
        .arg(
            Arg::new("generic_mod")
                .long("generic_mod")
                .short('g')
                .action(ArgAction::SetTrue)
                .help("Push generic mod in separate file"),
        )
        .arg(
            Arg::new("feature_group")
                .long("feature_group")
                .action(ArgAction::SetTrue)
                .help("Use group_name of peripherals as feature"),
        )
        .arg(
            Arg::new("feature_peripheral")
                .long("feature_peripheral")
                .action(ArgAction::SetTrue)
                .help("Use independent cfg feature flags for each peripheral"),
        )
        .arg(
            Arg::new("max_cluster_size")
                .long("max_cluster_size")
                .action(ArgAction::SetTrue)
                .help("Use array increment for cluster size"),
        )
        .arg(
            Arg::new("impl_debug")
                .long("impl_debug")
                .action(ArgAction::SetTrue)
                .help("implement Debug for readable blocks and registers"),
        )
        .arg(
            Arg::new("impl_debug_feature")
                .long("impl_debug_feature")
                .help("add feature gating for block and register debug implementation")
                .action(ArgAction::Set)
                .value_name("FEATURE"),
        )
        .arg(
            Arg::new("make_mod")
                .long("make_mod")
                .short('m')
                .action(ArgAction::SetTrue)
                .help("Create mod.rs instead of lib.rs, without inner attributes"),
        )
        .arg(
            Arg::new("strict")
                .long("strict")
                .short('s')
                .action(ArgAction::SetTrue)
                .help("Make advanced checks due to parsing SVD"),
        )
        .arg(
            Arg::new("pascal_enum_values")
                .long("pascal_enum_values")
                .action(ArgAction::SetTrue)
                .help("Use PascalCase in stead of UPPER_CASE for enumerated values"),
        )
        .arg(
            Arg::new("source_type")
                .long("source_type")
                .help("Specify file/stream format"),
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
    let path = &config.output_dir;

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
        let mut features = Vec::new();
        if config.feature_group {
            features.extend(
                util::group_names(&device)
                    .iter()
                    .map(|s| format!("{s} = []\n")),
            );
            let add_groups: Vec<_> = util::group_names(&device)
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect();
            features.push(format!("all-groups = [{}]\n", add_groups.join(",")))
        }
        if config.feature_peripheral {
            features.extend(
                util::peripheral_names(&device)
                    .iter()
                    .map(|s| format!("{s} = []\n")),
            );
            let add_peripherals: Vec<_> = util::peripheral_names(&device)
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
