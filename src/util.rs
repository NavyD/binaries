use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::{collections::HashSet, env::consts::ARCH, path::Path};

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use globset::Glob;
use log::info;
use log::trace;
use mime::Mime;
use tokio::process::Command;
use walkdir::WalkDir;

/// get strings of [ARCH][std::env::consts::ARCH].
///
/// [ref: zinit/zinit-install.zsh](https://github.com/zdharma-continuum/zinit/blob/c888917edbafa3772870ad1f320da7a5f169cc6f/zinit-install.zsh#L1453)
///
/// like this:
///
/// ```sh
/// i386    "((386|686|linux32|x86*(#e))~*x86_64*)"
/// i686    "((386|686|linux32|x86*(#e))~*x86_64*)"
/// x86_64  "(x86_64|amd64|intel|linux64)"
/// amd64   "(x86_64|amd64|intel|linux64)"
/// aarch64 "aarch64"
/// aarch64-2 "arm"
/// linux   "(linux|linux-gnu)"
/// darwin  "(darwin|mac|macos|osx|os-x)"
/// cygwin  "(windows|cygwin|[-_]win|win64|win32)"
/// windows "(windows|cygwin|[-_]win|win64|win32)"
/// msys "(windows|msys|cygwin|[-_]win|win64|win32)"
/// armv7l  "(arm7|armv7)"
/// armv7l-2 "arm7"
/// armv6l  "(arm6|armv6)"
/// armv6l-2 "arm"
/// armv5l  "(arm5|armv5)"
/// armv5l-2 "arm"
/// ```
pub fn get_archs() -> HashSet<String> {
    match ARCH {
        "x86" => vec!["386", "686", "linux32"],
        "x86_64" => vec!["x86_64", "amd64", "intel", "linux64"],
        "aarch64" => vec!["arm64"],
        _ => vec![],
    }
    .into_iter()
    .chain([ARCH])
    .map(|s| s.trim().to_string())
    .collect::<HashSet<_>>()
}

pub fn extract<P>(from: P, to: P, content_type: Option<&str>) -> Result<()>
where
    P: AsRef<Path>,
{
    let (from, to) = (from.as_ref(), to.as_ref());
    if let Some(ty) = content_type {
        return ex(from, to, &ty.parse()?).map_err(Into::into);
    }

    for ty in mime_guess::from_path(from) {
        if let Err(e) = ex(from, to, &ty) {
            info!(
                "failed to extract {} with mime {}: {}",
                from.display(),
                &ty,
                e
            );
        } else {
            return Ok(());
        }
    }

    bail!("failed to extract {}: all mimes tried", from.display());
}

pub fn ex<P>(from: P, to: P, content_type: &Mime) -> Result<()>
where
    P: AsRef<Path>,
{
    let (from, to) = (from.as_ref(), to.as_ref());

    trace!(
        "extracting {} to {} with mime: {}",
        from.display(),
        to.display(),
        content_type
    );
    todo!()
}

pub fn find_one_exe_with_glob(glob_pat: &str, base: impl AsRef<Path>) -> Result<PathBuf> {
    let base = base.as_ref();

    let glob = Glob::new(glob_pat)?.compile_matcher();
    let paths = WalkDir::new(base)
        .into_iter()
        .filter_entry(|entry| glob.is_match(entry.path()))
        .collect::<Result<Vec<_>, _>>()?;

    let path = if paths.is_empty() {
        bail!(
            "not found exe files with glob {} in {}",
            glob_pat,
            base.display()
        );
    } else if paths.len() > 1 {
        let paths = paths
            .iter()
            .filter(|p| {
                p.metadata()
                    // is executable
                    .map_or(false, |data| data.permissions().mode() & 0o111 != 0)
            })
            .collect::<Vec<_>>();
        if paths.len() != 1 {
            bail!(
                "failed to get exe file with glob {} in {}: {:?}",
                glob_pat,
                base.display(),
                paths
            );
        }
        paths[0]
    } else {
        &paths[0]
    }
    .path()
    .to_path_buf();

    Ok(path)
}

pub async fn run_cmd(cmd: &str, workdir: impl AsRef<Path>) -> Result<()> {
    let args = shell_words::split(cmd)?;
    if args.is_empty() {
        bail!("empty args: {}", cmd);
    }
    let mut child = Command::new(&args[0])
        .current_dir(workdir)
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let output = child.wait_with_output().await?;
    if !output.status.success() {
        bail!(
            "failed to run a command `{}` status {}. stdout: {}\nstderr: {}",
            cmd,
            output.status,
            std::str::from_utf8(&output.stdout)?,
            std::str::from_utf8(&output.stderr)?,
        );
    }
    Ok(())
}
