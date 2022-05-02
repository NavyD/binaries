use std::env::consts::OS;
use std::fmt::Display;
use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::{env::consts::ARCH, path::Path};

use anyhow::bail;
use anyhow::{anyhow, Result};
use globset::GlobBuilder;
use log::{debug, error, log_enabled, trace};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::json;
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
pub fn find_one_bin_with_glob(base: impl AsRef<Path>, glob_pat: &str) -> Result<PathBuf> {
    let base = base.as_ref();
    trace!(
        "finding one bin with glob {} in {}",
        glob_pat,
        base.display()
    );
    let glob = GlobBuilder::new(glob_pat)
        .literal_separator(true)
        .build()
        .map(|g| g.compile_matcher())?;

    let paths = WalkDir::new(base)
        // exclude the root: base
        .min_depth(1)
        .into_iter()
        .filter(|entry| entry.as_ref().map_or(false, |e| glob.is_match(e.path())))
        .collect::<Result<Vec<_>, _>>()?;
    match paths.len() {
        1 => {
            use std::fs;
            let path = paths[0].path().to_owned();
            debug!("found a bin file {} in {}", path.display(), base.display());

            const EXEC: u32 = 0o0111;
            // set permission exec
            fs::metadata(&path)
                .map(|d| d.permissions())
                .and_then(|mut perm| {
                    let old = perm.mode();
                    // if old & EXEC != 0 {
                    //     return Ok(())
                    // }
                    let new = perm.mode() | EXEC;
                    perm.set_mode(new);

                    trace!(
                        "set new mode {:#o} +x from old mode {:#o} for {}",
                        new,
                        old,
                        path.display()
                    );
                    fs::set_permissions(&path, perm)
                })?;

            Ok(path)
        }
        0 => {
            error!(
                "not found exe files with glob {} in {}",
                glob_pat,
                base.display()
            );
            bail!("not found one bin file");
        }
        len => {
            if log_enabled!(log::Level::Error) {
                error!(
                    "found {} bin files in {} by bin glob `{}`: {}",
                    len,
                    base.display(),
                    glob_pat,
                    paths
                        .iter()
                        .map(|p| p.path().display().to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            bail!("found multple bin files");
        }
    }
}

pub async fn run_cmd(cmd: &str, work_dir: impl AsRef<Path>) -> Result<()> {
    let args = shell_words::split(cmd)?;
    if args.is_empty() {
        bail!("empty args: {}", cmd);
    }
    trace!(
        "running command `{}` in word dir {}",
        cmd,
        work_dir.as_ref().display()
    );
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

pub fn platform_values(mut val: serde_json::Value) -> Result<serde_json::Value> {
    let mut base = json!({
        "os": OS,
        "arch": ARCH,
        "target_env": get_target_env(),
    });
    base.as_object_mut()
        .and_then(|o| val.as_object_mut().map(|v| o.append(v)))
        .ok_or_else(|| anyhow!("val is not a object: {}", val))?;
    Ok(base)
}

pub fn get_target_env() -> &'static str {
    #[cfg(target_env = "gnu")]
    {
        "gnu"
    }
    #[cfg(target_env = "musl")]
    {
        "musl"
    }
    #[cfg(target_env = "msvc")]
    {
        "msvc"
    }
}

#[derive(Debug, Clone, Default)]
pub struct Templater {
    h: Arc<Mutex<handlebars::Handlebars<'static>>>,
}

impl Templater {
    pub fn render(&self, template: &str, data: &(impl Serialize + Display)) -> Result<String> {
        trace!("rendering template `{}` with data: {}", template, data);
        self.h
            .lock()
            .render_template(template, data)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_one_exe_with_glob() -> Result<()> {
        let a = find_one_bin_with_glob("tests", "**/bin_exe")?;
        assert_eq!(a.file_name().and_then(|a| a.to_str()), Some("bin_exe"));
        Ok(())
    }

    #[test]
    fn test_val() -> Result<()> {
        let val = platform_values(json!({
            "name": "a",
            "repo": "b",
        }))?;
        assert_eq!(val["name"], "a");
        assert_eq!(val["repo"], "b");
        #[cfg(target_os = "linux")]
        {
            assert_eq!(val["os"], "linux");
        }
        Ok(())
    }
}
