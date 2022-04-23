use std::{
    fs::{self, create_dir_all, File, Permissions},
    io::{self, Read, Seek},
    os::unix::prelude::PermissionsExt,
    path::Path,
};

use anyhow::{anyhow, bail, Result};
use flate2::read::GzDecoder;
use log::{debug, info, trace};
use mime::Mime;
use once_cell::sync::Lazy;
use tar::Archive;
use tokio::fs as afs;
use zip::ZipArchive;

use crate::util::run_cmd;

pub async fn decompress<P>(from: P, to: P, cmd: Option<&str>) -> Result<()>
where
    P: AsRef<Path>,
{
    let (from, to) = (from.as_ref().to_owned(), to.as_ref().to_owned());

    if !afs::metadata(&from)
        .await
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        bail!("src {} is not a file", from.display());
    }

    match afs::metadata(&to).await {
        Ok(d) if !d.is_dir() => {
            bail!("target {} is not a dir", to.display());
        }
        Err(_) => afs::create_dir_all(&to).await?,
        _ => {}
    }

    if afs::read_dir(&to).await?.next_entry().await?.is_some() {
        info!(
            "a non empty directory {} was found when decompress",
            to.display()
        );
        return Ok(());
    }

    if let Some(cmd) = cmd {
        let word_dir = from
            .parent()
            .ok_or_else(|| anyhow!("not found parent dir for: {}", from.display()))?;

        run_cmd(cmd, &word_dir).await?;

        if afs::read_dir(&to).await?.next_entry().await?.is_none() {
            bail!(
                "empty directory {} after decompression by run command: {}",
                to.display(),
                cmd
            );
        }
        return Ok(());
    }

    tokio::task::spawn_blocking(move || extract(from, to)).await??;
    Ok(())
}

fn extract<P>(from: P, to: P) -> Result<()>
where
    P: AsRef<Path>,
{
    let (from, to) = (from.as_ref(), to.as_ref());

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

fn ex<P>(from: P, to: P, content_type: &Mime) -> Result<()>
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
        "application/gzip" => ex_gzip(from, to)?,
        _ => bail!("unsupported compress type: {}", content_type),
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
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, Permissions::from_mode(mode)).unwrap();
            }
        }
    }

    Ok(())
}

fn ex_gzip<P: AsRef<Path>>(from: P, to: P) -> Result<()> {
    let file = fs::File::open(&from)?;
    let mut gz_read = GzDecoder::new(file);
    let filename = from
        .as_ref()
        .file_stem()
        .and_then(|p| p.to_str())
        .ok_or_else(|| anyhow!("no filename"))?;
    let to_file_path = to.as_ref().join(filename);
    trace!(
        "extracting gzip to {} from {}",
        to_file_path.display(),
        from.as_ref().display()
    );
    io::copy(&mut gz_read, &mut fs::File::create(&to_file_path)?)?;

    let xtar = "application/x-tar".parse::<Mime>()?;
    if mime_guess::from_path(&to_file_path)
        .iter()
        .any(|x| x == xtar)
    {
        let mut archive = Archive::new(fs::File::open(&to_file_path)?);
        trace!(
            "unpack tar to {} from {}",
            to.as_ref().display(),
            to_file_path.display(),
        );
        archive.unpack(to)?;

        fs::remove_file(&to_file_path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use futures_util::StreamExt;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    use super::*;

    #[tokio::test]
    async fn test_gzip_to_one() -> Result<()> {
        let url = "https://github.com/Dreamacro/clash/releases/download/v1.10.0/clash-linux-amd64-v1.10.0.gz".parse::<url::Url>()?;
        let filename = url
            .path()
            .parse::<PathBuf>()
            .ok()
            .and_then(|p| {
                p.file_name()
                    .and_then(|s| s.to_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| panic!("not found filename for url: {}", url));

        let mut stream = reqwest::get(url.as_ref()).await?.bytes_stream();
        let root = tempdir()?;
        let from = root.path().join(filename);
        let to = root.path();
        let mut file = tokio::fs::File::create(&from).await?;
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }

        ex_gzip(from.as_path(), to)?;

        let target = to.join("clash-linux-amd64-v1.10.0");
        assert!(target.is_file());
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

    #[test]
    fn test_gzip() -> Result<()> {
        let zip_path = "tests/a.tar.gz".parse::<PathBuf>()?;
        let root = tempdir()?;
        assert!(!root.path().join("a").is_dir());
        ex_gzip(zip_path.as_path(), root.path())?;

        assert!(root.path().join("a").is_dir());
        assert!(root.path().join("a/a.txt").is_file());
        assert!(root.path().join("a/b/a.txt").is_file());
        Ok(())
    }
}
