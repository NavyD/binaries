use std::fs::{self, create_dir_all, File, Permissions};
use std::io::{self, Read, Seek};
use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::{collections::HashSet, env::consts::ARCH, path::Path};

use anyhow::bail;
use anyhow::Result;
use flate2::read::GzDecoder;
use globset::Glob;
use log::trace;
use log::{debug, info};
use mime::Mime;
use once_cell::sync::Lazy;
use tar::Archive;
use tokio::process::Command;
use walkdir::WalkDir;
use zip::ZipArchive;

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
        _ => panic!("unsupported arch: {}", ARCH),
    }
    .into_iter()
    .chain([ARCH])
    .map(|s| s.trim().to_string())
    .collect::<_>()
}

pub fn extract<P>(from: P, to: P, content_type: Option<&str>) -> Result<()>
where
    P: AsRef<Path>,
{
    let (from, to) = (from.as_ref(), to.as_ref());
    if let Some(ty) = content_type {
        return ex(from, to, &ty.parse()?).map_err(Into::into);
    }

    let mimes = mime_guess::from_path(from);
    for ty in &mimes {
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

    bail!(
        "failed to extract {}: all mimes tried: {:?}",
        from.display(),
        mimes
    );
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

    match content_type.as_ref() {
        "application/zip" => ex_zip(File::open(from)?, to)?,
        "application/gzip" => ex_gzip(File::open(from)?, to)?,
        _ => bail!("unsupported compress type: {}", content_type),
    }

    Ok(())
}

/// 尝试从base中找到一个符合glob_pat的可执行的bin文件path
///
/// # Error
///
/// * 如果未匹配任何path
/// * 如果匹配到多个可执行的path
pub fn find_one_exe_with_glob(base: impl AsRef<Path>, glob_pat: &str) -> Result<PathBuf> {
    let base = base.as_ref();

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
                    .map_or(false, |data| data.permissions().mode() & 0o111 != 0)
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

pub static SUPPORTED_CONTENT_TYPES: Lazy<[Mime; 2]> = Lazy::new(|| {
    [
        "application/zip".parse::<Mime>().expect("mime zip"),
        "application/gzip".parse::<Mime>().expect("mime gzip"),
    ]
});

fn ex_zip(from: impl Read + Seek, to: impl AsRef<Path>) -> Result<()> {
    let mut archive = ZipArchive::new(from)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = if let Some(path) = file.enclosed_name().map(|p| to.as_ref().join(p)) {
            path
        } else {
            continue;
        };

        {
            let comment = file.comment();
            if !comment.is_empty() {
                debug!("File {} comment: {}", i, comment);
            }
        }

        if (*file.name()).ends_with('/') {
            debug!("File {} extracted to \"{}\"", i, outpath.display());
            create_dir_all(&outpath)?;
        } else {
            debug!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.display(),
                file.size()
            );
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    create_dir_all(&p)?;
                }
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }

        // Get and Set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, Permissions::from_mode(mode)).unwrap();
            }
        }
    }

    Ok(())
}

fn ex_gzip<P: AsRef<Path>, R: Read>(from: R, to: P) -> Result<()> {
    let tar = GzDecoder::new(from);
    let mut archive = Archive::new(tar);
    archive.unpack(to)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_find_one_exe_with_glob() -> Result<()> {
        let a = find_one_exe_with_glob("tests", "**/bin_exe")?;
        assert_eq!(a.file_name().and_then(|a| a.to_str()), Some("bin_exe"));
        Ok(())
    }

    #[test]
    fn test_gzip() -> Result<()> {
        let zip_path = "tests/a.tar.gz".parse::<PathBuf>()?;
        let root = tempdir()?;
        assert!(!root.path().join("a").is_dir());
        ex_gzip(File::open(zip_path)?, root.path())?;

        assert!(root.path().join("a").is_dir());
        assert!(root.path().join("a/a.txt").is_file());
        assert!(root.path().join("a/b/a.txt").is_file());
        Ok(())
    }

    #[test]
    fn test_zip() -> Result<()> {
        let zip_path = "tests/a.zip".parse::<PathBuf>()?;
        let root = tempdir()?;
        assert!(!root.path().join("a").is_dir());
        ex_zip(File::open(zip_path)?, root.path())?;

        assert!(root.path().join("a").is_dir());
        assert!(root.path().join("a/a.txt").is_file());
        assert!(root.path().join("a/b/a.txt").is_file());
        Ok(())
    }

    fn create_zip() -> Result<PathBuf> {
        todo!()
    }
}
