use core::fmt;
use std::{collections::HashSet, num::NonZeroU8};

use chrono::{DateTime, Utc};
use crates_io_api::Version;
use log::{error, info};
use thiserror::Error;

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub version: String,
    /* source: Option<String>,
    dependencies: Option<HashMap<String, Value>>, */
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = \"={}\"", self.name, self.version)
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to read Cargo.lock")]
    ReadCargoLock(#[from] std::io::Error),
    #[error("Failed to parse Cargo.lock")]
    ParseCargoLock(#[from] cargo_lock::Error),
    #[error("Failed to fetch from crates.io")]
    Reqwest(#[from] crates_io_api::Error),
    #[error("At least for one crate there was no appropriate version found")]
    NoAppropriateVersion,
}
type Result<T> = std::result::Result<T, Error>;

/// Get all crate names of transitive dependencies from in Cargo.lock file up to `dependency_level`
pub fn get_dependencies(
    dependency_level: Option<NonZeroU8>,
    dependency_tree: &cargo_lock::dependency::Tree,
) -> HashSet<&str> {
    let mut crate_names = HashSet::new();

    // initialize the worklist with the root nodes
    let mut worklist: Vec<petgraph::prelude::NodeIndex> = dependency_tree
        .graph()
        .externals(petgraph::Direction::Incoming)
        .collect();

    let mut level: u8 = 0;
    while !worklist.is_empty() {
        let mut next_level_worklist = vec![];
        let mut dependencies_current_level = HashSet::new();
        // iterate all dependencies on the current level
        for node_index in worklist {
            let package: &cargo_lock::Package = &dependency_tree.graph()[node_index];
            dependencies_current_level.insert(package.name.as_str());
            // push the transitive dependencies on the next level to the worklist
            for child in dependency_tree
                .graph()
                .neighbors_directed(node_index, petgraph::Direction::Outgoing)
            {
                next_level_worklist.push(child);
            }
        }
        info!(
            "dependencies on level {}: {}",
            level,
            String::from_iter(
                dependencies_current_level
                    .iter()
                    .map(|s| format!("{}, ", s))
            )
        );

        worklist = next_level_worklist;

        if level > 0 {
            match dependency_level {
                Some(dependency_level) => {
                    if level >= dependency_level.get() {
                        return dependencies_current_level;
                    }
                }

                None => crate_names.extend(dependencies_current_level),
            }
        }

        level = match level.checked_add(1) {
            Some(l) => l,
            None => {
                error!("more than 255 levels of dependencies found, aborting");
                break;
            }
        };
    }

    crate_names
}

fn find_appropriate_version(
    crate_name: &str,
    mut versions: Vec<Version>,
    date: DateTime<Utc>,
) -> std::result::Result<Package, String> {
    // sort versions by release date
    versions.sort_unstable_by_key(|version| version.updated_at);

    // find the last version that has been published before `date`
    match versions
        .iter()
        .rev()
        .find(|version| version.updated_at < date && !version.yanked)
    {
        Some(version) => Ok(Package {
            version: version.num.clone(),
            name: (*crate_name).to_owned(),
        }),
        None => Err(format!(
            "No version of crate {} found before date. Oldest unyanked version is: {}",
            (*crate_name).to_owned(),
            versions
                .iter()
                .find(|version| !version.yanked)
                .map(|v| format!("{} ({})", v.num, v.updated_at.format("%Y-%m-%d")))
                .unwrap_or_else(|| "no known versions at all?".to_owned()),
        )),
    }
}

/// For every defined package in `cargo_lock`, find the version that has been published before `date`
pub async fn get_downgraded_dependencies(
    crate_names: &[&str],
    date: DateTime<Utc>,
) -> Result<Vec<Package>> {
    info!(
        "downgrading the following {} dependencies to {}: {}",
        crate_names.len(),
        date,
        crate_names.join(", ")
    );
    let cratesio_api_client = crates_io_api::AsyncClient::new(
        "downgrade crawler (https://github.com/obraunsdorf/cargo-downgrade)", // TODO link to github
        std::time::Duration::from_millis(1000),
    )
    .unwrap();

    // sequentially fetch the version information for all packages since we connect to the crates.io API only every second
    let mut downgraded_dependencies = vec![];
    for crate_name in crate_names {
        info!("fetching infos for crate {}", crate_name);
        let crate_data = cratesio_api_client.get_crate(crate_name).await?;
        match find_appropriate_version(crate_name, crate_data.versions, date) {
            Ok(package) => downgraded_dependencies.push(package),
            Err(err) => {
                error!("{}", err);
            }
        }
    }

    Ok(downgraded_dependencies)
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_get_downgraded_dependencies() {
        let datetime: DateTime<Utc> = DateTime::parse_from_rfc2822("22 Feb 2021 23:16:09 GMT")
            .unwrap()
            .with_timezone(&Utc);
        let crate_names = vec!["serde"];
        let downgraded_dependencies = get_downgraded_dependencies(&crate_names, datetime)
            .await
            .unwrap();
        assert_eq!(downgraded_dependencies[0].version, "1.0.123");
    }
}
