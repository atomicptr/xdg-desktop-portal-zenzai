use std::{fs::File, io::Read};

use anyhow::{Context, Result, anyhow, ensure};
use ashpd::{
    Error,
    desktop::{
        ResponseError,
        file_chooser::{FileFilter, SelectedFiles},
    },
};
use url::Url;
use zenzai_core::ipc::MAX_LAUNCHER_ICON_BYTES as MAX_ICON_BYTES;

use crate::icon::{self, Icon};

#[derive(Debug, Clone)]
pub struct Selection {
    pub bytes: Vec<u8>,
    pub preview: Icon,
}

pub async fn pick() -> Result<Option<Selection>> {
    let selected = match SelectedFiles::open_file()
        .title("Choose launcher icon")
        .accept_label("Select")
        .modal(true)
        .multiple(false)
        .filter(
            FileFilter::new("Launcher icons")
                .mimetype("image/png")
                .mimetype("image/jpeg")
                .mimetype("image/svg+xml"),
        )
        .send()
        .await
        .and_then(|request| request.response())
    {
        Ok(selected) => selected,
        Err(Error::Response(ResponseError::Cancelled)) => return Ok(None),
        Err(err) => return Err(err).context("could not open file chooser"),
    };
    let Some(uri) = selected.uris().first() else {
        return Ok(None);
    };
    let path = Url::parse(uri.as_str())?
        .to_file_path()
        .map_err(|()| anyhow!("the selected icon must be a local file"))?;

    tokio::task::spawn_blocking(move || {
        let file = File::open(path).context("could not open icon")?;
        ensure!(file.metadata()?.is_file(), "select a regular image file");
        let mut bytes = Vec::new();
        file.take(MAX_ICON_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_ICON_BYTES, "icon must not exceed 16 MiB");
        let preview = icon::decode(&bytes)?.context("the selected icon is empty")?;
        Ok(Some(Selection { bytes, preview }))
    })
    .await?
}
