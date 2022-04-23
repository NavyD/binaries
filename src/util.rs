use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::{env::consts::ARCH, path::Path};

use anyhow::bail;
use anyhow::Result;
use globset::Glob;
use log::trace;
use log::{debug, error};
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
pub fn get_archs() -> Vec<String> {
    match ARCH {
        "x86" => vec!["386", "686", "linux32"],
        "x86_64" => vec!["x86_64", "amd64", "intel", "linux64"],
        "aarch64" => vec!["arm64"],
        s => panic!("unsupported arch: {}", s),
    }
    .into_iter()
    .chain([ARCH])
    .map(|s| s.trim().to_string())
    .collect::<_>()
}

/// 尝试从base中找到一个符合glob_pat的可执行的bin文件path
///
/// # Error
///
/// * 如果未匹配任何path
/// * 如果匹配到多个可执行的path
pub fn find_one_exe_with_glob(base: impl AsRef<Path>, glob_pat: &str) -> Result<PathBuf> {
    let base = base.as_ref();
    trace!("finding one with glob {} in {}", glob_pat, base.display());

    let glob = Glob::new(glob_pat)?.compile_matcher();
    let paths = WalkDir::new(base)
        .into_iter()
        .filter(|entry| entry.as_ref().map_or(false, |e| glob.is_match(e.path())))
        .collect::<Result<Vec<_>, _>>()?;

    let path = if paths.is_empty() {
        bail!(
            "not found exe files with glob {} in {}",
            glob_pat,
            base.display()
        );
    } else if paths.len() > 1 {
        debug!("found {} paths for exe glob: {}", paths.len(), glob_pat);
        let paths = paths
            .iter()
            .filter(|p| {
                p.metadata()
                    // is executable
                    .map_or(false, |data| {
                        data.is_file() && data.permissions().mode() & 0o111 != 0
                    })
            })
            .collect::<Vec<_>>();
        trace!("found {} paths by filter exe permissions", paths.len());
        if paths.len() != 1 {
            error!(
                "failed to get exe file with glob {} in {}. executable paths: {:?}",
                glob_pat,
                base.display(),
                paths
            );
            bail!("not found a path for exe glob: {}", glob_pat);
        }
        paths[0]
    } else {
        &paths[0]
    }
    .path()
    .to_path_buf();

    Ok(path)
}

pub async fn run_cmd(cmd: &str, work_dir: impl AsRef<Path>) -> Result<()> {
    let args = shell_words::split(cmd)?;
    if args.is_empty() {
        bail!("empty args: {}", cmd);
    }
    let child = Command::new(&args[0])
        .current_dir(work_dir)
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let output = child.wait_with_output().await?;
    trace!(
        "`{}` stdout: {}, stderr: {}",
        cmd,
        std::str::from_utf8(&output.stdout)?,
        std::str::from_utf8(&output.stderr)?,
    );
    if !output.status.success() {
        bail!("failed to run a command `{}` status {}", cmd, output.status,);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_one_exe_with_glob() -> Result<()> {
        let a = find_one_exe_with_glob("tests", "**/bin_exe")?;
        assert_eq!(a.file_name().and_then(|a| a.to_str()), Some("bin_exe"));
        Ok(())
    }
}
