use materialcraft_core::Package;
use std::{collections::BTreeMap, sync::Arc};

pub(crate) const STANDARD_PACKAGE: &str = "sbs://functions.sbs";

pub(crate) struct FunctionLibrary {
    pub packages: BTreeMap<String, Arc<Package>>,
    pub warnings: Vec<String>,
}

pub(crate) fn load() -> Result<FunctionLibrary, String> {
    let bytes = materialcraft_formats::sbs::library::read(STANDARD_PACKAGE)?;
    let mut read =
        materialcraft_formats::sbs::read_sbs(&bytes, STANDARD_PACKAGE, &crate::file_cmds::linked_files(STANDARD_PACKAGE)).map_err(|error| error.to_string())?;
    crate::linked_files::normalize_references(&mut read.pkg, STANDARD_PACKAGE);
    read.warnings.extend(crate::linked_files::load(&mut read.pkg));

    let mut packages = std::mem::take(&mut read.pkg.dependencies);
    packages.insert(STANDARD_PACKAGE.into(), Arc::new(read.pkg));
    Ok(FunctionLibrary { packages, warnings: read.warnings })
}
