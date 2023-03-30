use crate::dockerfile;
use crate::dockerfile::download_libc_ld_for_docker_tag;
use crate::maybe_visit_libc;
use crate::opts;
use crate::patch_bin;
use crate::set_bin_exec;
use crate::set_ld_exec;
use crate::solvepy;
use crate::visit_dockerfile;
use crate::Opts;

use ex::io;
use snafu::ResultExt;
use snafu::Snafu;

/// Top-level `pwninit` error
#[derive(Debug, Snafu)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    #[snafu(display("failed setting binary executable: {}", source))]
    SetBinExec { source: io::Error },

    #[snafu(display(
        "failed locating provided files (binary, libc, linker, Dockerfile): {}",
        source
    ))]
    Find { source: opts::Error },

    #[snafu(display("failed setting linker executable: {}", source))]
    SetLdExec { source: io::Error },

    #[snafu(display("failed patching binary: {}", source))]
    PatchBin { source: patch_bin::Error },

    #[snafu(display("failed making template solve script: {}", source))]
    Solvepy { source: solvepy::Error },

    #[snafu(display("Failed to extract files from docker image: {}", source))]
    Dockerfile { source: dockerfile::Error },
}

pub type Result = std::result::Result<(), Error>;

/// Run `pwninit` with specified options
pub fn run(opts: Opts) -> Result {
    // Detect unspecified files
    let opts = opts.find_if_unspec().context(FindSnafu)?;

    // Print detected files
    opts.print();
    println!();

    set_bin_exec(&opts).context(SetBinExecSnafu)?;
    // We might have to pull a libc and ld from a Docker image
    if opts.libc.is_none() || opts.ld.is_none() {
        // Docker tags get priority, since those have to be explicitly set.
        if let Some(docker_tag) = opts.docker_tag.as_ref() {
            download_libc_ld_for_docker_tag(&docker_tag).context(DockerfileSnafu)?;
        } else if let Some(dockerfile) = opts.dockerfile.as_ref() {
            visit_dockerfile(dockerfile).context(DockerfileSnafu)?;
        }
    }

    // Redo detection in case libc or ld was pulled from Dockerfile
    let opts = opts.find_if_unspec().context(FindSnafu)?;

    maybe_visit_libc(&opts);

    // Redo detection in case the ld was downloaded
    let opts = opts.find_if_unspec().context(FindSnafu)?;

    set_ld_exec(&opts).context(SetLdExecSnafu)?;

    if !opts.no_patch_bin {
        patch_bin::patch_bin(&opts).context(PatchBinSnafu)?;
    }

    if !opts.no_template {
        solvepy::write_stub(&opts).context(SolvepySnafu)?;
    }

    Ok(())
}
