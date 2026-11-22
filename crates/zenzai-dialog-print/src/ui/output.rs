use std::path::{Path, PathBuf};

use ashpd::{
    Error,
    desktop::{
        ResponseError,
        file_chooser::{FileFilter, SelectedFiles},
    },
};

use dirs::home_dir;
use url::Url;

const HOME_PREFIX: &str = "~";
const PDF_FILTER_NAME: &str = "PDF documents";
const PDF_MIME_TYPE: &str = "application/pdf";
const PICKER_TITLE: &str = "Save as PDF";

pub async fn pick(current: PathBuf) -> Result<Option<PathBuf>, String> {
    let request = SelectedFiles::save_file()
        .title(PICKER_TITLE)
        .modal(true)
        .filter(FileFilter::new(PDF_FILTER_NAME).mimetype(PDF_MIME_TYPE))
        .current_name(current.file_name().and_then(|name| name.to_str()))
        .current_folder::<&Path>(current.parent())
        .map_err(|err| err.to_string())?;

    let selected = match request.send().await.and_then(|request| request.response()) {
        Ok(selected) => selected,
        Err(Error::Response(ResponseError::Cancelled)) => return Ok(None),
        Err(err) => return Err(format!("could not open file chooser: {err}")),
    };

    let Some(uri) = selected.uris().first() else {
        return Ok(None);
    };

    Url::parse(uri.as_str())
        .ok()
        .and_then(|url| url.to_file_path().ok())
        .map(Some)
        .ok_or_else(|| format!("not a local file: {uri}"))
}

pub fn display(path: &Path) -> String {
    if let Some(home) = home_dir()
        && let Ok(relative) = path.strip_prefix(home)
    {
        return Path::new(HOME_PREFIX).join(relative).display().to_string();
    }

    path.display().to_string()
}

pub fn expand(path: &str) -> PathBuf {
    match (Path::new(path).strip_prefix(HOME_PREFIX), home_dir()) {
        (Ok(relative), Some(home)) => home.join(relative),
        _ => PathBuf::from(path),
    }
}
