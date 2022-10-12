#![recursion_limit = "128"]

use log::{error, info};
use std::path::{Path, PathBuf};

use std::fs::File;
use std::io::{Read, Write};
use std::process;

use anyhow::{Context, Result};
use clap::{builder::TypedValueParser, Parser};
use clap_verbosity_flag::Verbosity;
use serde::Deserialize;

use svd2rust::{
    generate, load_from,
    util::{self, build_rs, Config, SourceType, Target},
};

#[derive(Parser, Debug)]
#[command(author, version = concat!(
    env!("CARGO_PKG_VERSION"),
    include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
), about, long_about = None)]
struct Args {
    /// Input SVD file
    #[arg(short, long, value_name = "FILE")]
    input: Option<String>,
    /// Directory to place generated files
    #[arg(short, long, value_name = "PATH", default_value_t = String::from("."))]
    output_dir: String,
    /// Config TOML file
    #[arg(short, long, value_name = "TOML_FILE")]
    config: Option<String>,
    /// Target architecture
    #[arg(
        long,
        value_name = "ARCH",
        default_value_t = Target::default(),
        value_parser = clap::builder::PossibleValuesParser::new(["cortex-m", "msp430", "riscv", "xtensa-lx", "mips", "none"])
            .map(|s| Target::parse(&s).unwrap()),
    )]
    target: Target,
    /// Enable features only available to nightly rustc
    #[arg(long = "nightly")]
    nightly_features: bool,
    /// Use const generics to generate writers for same fields with different offsets
    #[arg(long)]
    const_generic: bool,
    /// Don't add alternateGroup name as prefix to register name
    #[arg(long)]
    ignore_groups: bool,
    /// Keep lists when generating code of dimElement, instead of trying to generate arrays
    #[arg(long)]
    keep_list: bool,
    /// Push generic mod in separate file
    #[arg(short, long)]
    generic_mod: bool,
    /// Use group_name of peripherals as feature
    #[arg(long)]
    feature_group: bool,
    /// Use independent cfg feature flags for each peripheral
    #[arg(long)]
    feature_peripheral: bool,
    /// Use array increment for cluster size
    #[arg(long)]
    max_cluster_size: bool,
    /// Create mod.rs instead of lib.rs, without inner attributes
    #[arg(short, long)]
    make_mod: bool,
    /// Make advanced checks due to parsing SVD
    #[arg(short, long)]
    strict: bool,
    /// Use PascalCase in stead of UPPER_CASE for enumerated values
    #[arg(long)]
    pascal_enum_values: bool,
    /// Use derive_more procedural macros to implement Deref and From
    #[arg(long)]
    derive_more: bool,
    /// Specify file/stream format
    #[arg(
        long,
        default_value_t = SourceType::default(),
        value_parser = clap::builder::PossibleValuesParser::new([
            "xml",
            #[cfg(feature = "yaml")] "yaml",
            #[cfg(feature = "json")] "json"
        ])
        .map(|s| SourceType::from_extension(&s).unwrap()),
    )]
    source_type: SourceType,
    #[command(flatten)]
    verbose: Verbosity,
}

#[derive(Deserialize)]
struct TomlArgs {
    log_level: log::LevelFilter,
    target: Target,
    source_type: SourceType,
}

fn run() -> Result<()> {
    let args = Args::parse();

    let mut input = String::new();
    let (input, source_type) = match &args.input {
        Some(file) => {
            File::open(&file)
                .context("svd2rust open the SVD file")?
                .read_to_string(&mut input)
                .context("svd2rust read the SVD file")?;
            (input, SourceType::from_path(Path::new(&file)))
        }
        None => {
            let stdin = std::io::stdin();
            stdin
                .lock()
                .read_to_string(&mut input)
                .context("svd2rust read from standard input")?;
            (input, args.source_type)
        }
    };

    let (source_type, target) = if let Some(config_path) = args.config {
        let mut config_file = File::open(&config_path).context("svd2rust open the SVD file")?;
        let mut config_content = String::new();
        config_file
            .read_to_string(&mut config_content)
            .context("svd2rust read config file")?;
        let toml_args: TomlArgs = toml::from_str(&config_content).context("svd2rust parse toml")?;
        env_logger::Builder::new()
            .filter_level(toml_args.log_level)
            .init();
        (toml_args.source_type, toml_args.target)
    } else {
        env_logger::Builder::new()
            .filter_level(args.verbose.log_level_filter())
            .init();
        (source_type, args.target)
    };

    let path = PathBuf::from(args.output_dir);

    let config = Config {
        target,
        nightly: args.nightly_features,
        generic_mod: args.generic_mod,
        make_mod: args.make_mod,
        const_generic: args.const_generic,
        ignore_groups: args.ignore_groups,
        keep_list: args.keep_list,
        strict: args.strict,
        pascal_enum_values: args.pascal_enum_values,
        derive_more: args.derive_more,
        feature_group: args.feature_group,
        feature_peripheral: args.feature_peripheral,
        max_cluster_size: args.max_cluster_size,
        output_dir: path.clone(),
        source_type,
    };

    info!("Parsing device from SVD file");
    let device = load_from(&input, &config)?;

    let mut device_x = String::new();
    info!("Rendering device");
    let items = generate::device::render(&device, &config, &mut device_x)
        .with_context(|| "svd2rust rendering device")?;

    let filename = if args.make_mod { "mod.rs" } else { "lib.rs" };
    let mut file = File::create(path.join(filename)).expect("Couldn't create output file");

    let data = items.to_string().replace("] ", "]\n");
    file.write_all(data.as_ref())
        .expect("svd2rust write code to lib.rs");

    if matches!(
        args.target,
        Target::CortexM | Target::Msp430 | Target::XtensaLX | Target::RISCV
    ) {
        writeln!(File::create(path.join("device.x"))?, "{}", device_x)?;
        writeln!(File::create(path.join("build.rs"))?, "{}", build_rs())?;
    }

    if args.feature_group || args.feature_peripheral {
        let mut features = Vec::new();
        if args.feature_group {
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
        if args.feature_peripheral {
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

fn main() {
    if let Err(ref e) = run() {
        error!("{e:?}");

        process::exit(1);
    }
}
