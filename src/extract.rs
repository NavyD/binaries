use std::path::Path;

use anyhow::{anyhow, bail, Result};
use futures_util::{FutureExt, StreamExt};
use tokio::fs as afs;

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
        bail!("{} is not a file", from.display());
    }
    let a = afs::read_dir(&to).then(|dir| async { dir.map(|mut d| d.next_entry()) });

    if !afs::metadata(&to)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        bail!("{} is not a dir or not a empty dir", to.display());
    }

    if let Some(cmd) = cmd {
        let word_dir = from
            .parent()
            .ok_or_else(|| anyhow!("not found parent dir for: {}", from.display()))?;

        // run_cmd(&cmd, &word_dir).await?;
    }

    todo!()
}

fn decompress_with_cmd<P>(from: P, to: P, cmd: &str) -> Result<()>
where
    P: AsRef<Path>,
{
    let (from, to) = (from.as_ref().to_owned(), to.as_ref().to_owned());

    let word_dir = from
        .parent()
        .ok_or_else(|| anyhow!("not found parent dir for: {}", from.display()))?;

    todo!()
}
