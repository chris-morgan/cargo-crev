use crate::{
    deps::{
        scan::{self, RequiredDetails},
        AccumulativeCrateDetails,
    },
    opts::{CrateSelector, CrateVerify, CrateVerifyCommon},
    Repo,
};
use anyhow::{bail, Result};
use crev_data::proof;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Details {
    pub verified: bool,
    pub loc: Option<u64>,
    pub geiger_count: Option<u64>,
    pub has_custom_build: bool,
    pub unmaintained: bool,
}

impl From<AccumulativeCrateDetails> for Details {
    fn from(details: AccumulativeCrateDetails) -> Self {
        Details {
            verified: details.verified,
            loc: details.loc,
            geiger_count: details.geiger_count,
            has_custom_build: details.has_custom_build,
            unmaintained: details.is_unmaintained,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CrateInfoDepOutput {
    pub details: Details,
    pub recursive_details: Details,
    pub dependencies: Vec<proof::PackageVersionId>,
    pub rev_dependencies: Vec<proof::PackageVersionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CrateInfoOutput {
    pub package: proof::PackageVersionId,
    #[serde(flatten)]
    pub deps: Option<CrateInfoDepOutput>,
    pub alternatives: HashSet<proof::PackageId>,
    // pub flags: proof::Flags,
}

pub fn get_crate_deps_info(
    pkg_id: cargo::core::PackageId,
    common_opts: CrateVerifyCommon,
    required_details: &RequiredDetails,
) -> Result<CrateInfoDepOutput> {
    let args = CrateVerify {
        common: common_opts,
        ..Default::default()
    };
    let scanner = scan::Scanner::new(CrateSelector::default(), &args)?;
    let events = scanner.run(required_details);

    let stats = events
        .into_iter()
        .find(|stats| stats.info.id == pkg_id)
        .expect("result");

    Ok(CrateInfoDepOutput {
        details: stats.details().accumulative_own.clone().into(),
        recursive_details: stats.details().accumulative_recursive.clone().into(),
        dependencies: stats.details().dependencies.clone(),
        rev_dependencies: stats.details().rev_dependencies.clone(),
    })
}

pub fn get_crate_info(
    root_crate: CrateSelector,
    common_opts: CrateVerifyCommon,
) -> Result<CrateInfoOutput> {
    if root_crate.name.is_none() {
        bail!("Crate selector required");
    }

    let local = crev_lib::Local::auto_create_or_open()?;
    let db = local.load_db()?;
    let trust_set = local.trust_set_for_id(
        common_opts.for_id.as_deref(),
        &common_opts.trust_params.clone().into(),
        &db,
    )?;

    let repo = Repo::auto_open_cwd(common_opts.cargo_opts.clone())?;
    let pkg_id = repo.find_pkgid_by_crate_selector(&root_crate)?;
    let crev_pkg_id = crate::cargo_pkg_id_to_crev_pkg_id(&pkg_id);
    Ok(CrateInfoOutput {
        package: crev_pkg_id.clone(),
        deps: if root_crate.unrelated {
            None
        } else {
            Some(get_crate_deps_info(
                pkg_id,
                common_opts,
                &RequiredDetails::none(),
            )?)
        },
        alternatives: db
            .get_pkg_alternatives(&crev_pkg_id.id)
            .iter()
            .filter(|(author, _)| trust_set.is_trusted(author))
            .map(|(_, id)| id)
            .cloned()
            .collect(),
        // flags: db
        //     .get_pkg_flags(&crev_pkg_id.id)
        //     .filter(|(author, _)| trust_set.contains_trusted(author))
        //     .map(|(_, flags)| flags)
        //     .fold(proof::Flags::default(), |acc, flags| acc + flags.clone()),
    })
}

pub fn print_crate_info(root_crate: CrateSelector, args: CrateVerifyCommon) -> Result<()> {
    let info = get_crate_info(root_crate, args)?;
    serde_yaml::to_writer(io::stdout(), &info)?;
    println!();

    Ok(())
}
