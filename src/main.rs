use chrono::DateTime;
use clap::{Parser, Subcommand};
use error_reporter::Report;
use futures::lock;
use std::{
    io::{self, Write},
    num::NonZeroU8,
    path::PathBuf,
    process::Command,
};

#[derive(Parser, Debug)]
struct CliArguments {
    /// Path to the Cargo.lock file.
    cargo_lock: Option<PathBuf>,

    /// Date to which the dependencies should be downgraded. In RFC 2822 format, e.g. "22 Feb 2021 23:16:09 GMT"
    #[clap(long, short)]
    date: String,

    /// Get the date from git
    #[clap(long, short)]
    git: bool,

    /// Actually run the downgrade
    #[clap(long, short)]
    run: bool,

    #[clap(subcommand)]
    modes: DowngradeModes,
}

#[derive(Subcommand, Debug)]
enum DowngradeModes {
    /// Downgrade all crate names of transitive dependencies in Cargo.lock file up to `dependency_level`
    All {
        /// Dependency level to which transitive dependencies of the crate should be downgraded.
        #[clap(long, short = 'l')]
        dependency_level: Option<NonZeroU8>,
    },

    /// Downgrade a list of specific crates
    This {
        /// Comma-separated list of crate names to downgrade
        #[clap(value_delimiter = ',', required = true)]
        crates: Vec<String>,
    },
}

fn get_timestamp_from_git() -> Option<DateTime<chrono::Utc>> {
    let mut input = Command::new("git");

    input.arg("show").arg("-s").arg("--format=%ct");
    let output = input.output().ok()?;
    let stdout = String::from_utf8(output.stdout).ok()?;
    let secs = stdout.trim().parse().ok()?;

    let datetime: DateTime<chrono::Utc> =
        DateTime::from_timestamp(secs, 0)?.with_timezone(&chrono::Utc);

    Some(datetime)
}

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let mut args = CliArguments::parse();

    args.run = true;
    args.git = true;

    let lock_path = match args.cargo_lock {
        Some(path) => path,
        None => {
            let mut path = std::env::current_dir().unwrap();
            path.push("Cargo.lock");
            path
        }
    };
    let cargo_lock = cargo_lock::Lockfile::load(lock_path).unwrap();
    let dependency_tree = cargo_lock.dependency_tree().unwrap();

    let crate_names = match &args.modes {
        DowngradeModes::All { dependency_level } => {
            cargo_downgrade::get_dependencies(*dependency_level, &dependency_tree)
                .into_iter()
                .collect()
        }
        DowngradeModes::This { crates } => {
            let mut crate_names = crates.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            // vector has to be sorted for dedup to work
            crate_names.sort();
            crate_names.dedup();
            crate_names
        }
    };

    let datetime = if args.git {
        get_timestamp_from_git().unwrap()
    } else {
        DateTime::parse_from_rfc2822(&args.date)
            .unwrap()
            .with_timezone(&chrono::Utc)
    };

    // cargo update -p <package_name> --precise <version>

    match cargo_downgrade::get_downgraded_dependencies(&crate_names, datetime).await {
        Ok(downgraded_dependencies) => {
            for dep in downgraded_dependencies {
                if args.run {
                    let output = Command::new("cargo")
                        .arg("update")
                        .arg("-p")
                        .arg(dep.name)
                        .arg("--precise")
                        .arg(dep.version)
                        .output()
                        .unwrap();

                    io::stdout().write_all(&output.stdout).unwrap();
                    io::stderr().write_all(&output.stderr).unwrap();
                } else {
                    println!("{}", dep);
                }
            }
        }
        Err(err) => {
            eprintln!("Error: {}", Report::new(err));
            std::process::exit(1);
        }
    }
}
