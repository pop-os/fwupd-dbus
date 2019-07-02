use crate::dbus_helpers::*;
use crate::{Client, DBusEntry};
use dbus::arg::RefArg;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Seek, SeekFrom},
    iter::FromIterator,
    path::{Path, PathBuf},
};

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(display = "fwupd client errored when updating metadata for remote")]
    Client(#[error(cause)] crate::Error),
    #[error(display = "failed to copy firmware metadata from remote")]
    Copy(#[error(cause)] reqwest::Error),
    #[error(display = "failed to create parent directories for the remote's metadata cache")]
    CreateParent(#[error(cause)] io::Error),
    #[error(display = "remote returned error when fetching firmware metadata")]
    Get(#[error(cause)] reqwest::Error),
    #[error(
        display = "unable to open cached firmware metadata ({:?}) for remote",
        _1
    )]
    Open(#[error(cause)] io::Error, PathBuf),
    #[error(
        display = "failed to read the cached firmware metadata ({:?}) for remote",
        _1
    )]
    Read(#[error(cause)] io::Error, PathBuf),
    #[error(display = "failed to seek to beginning of firmware file")]
    Seek(#[error(cause)] io::Error),
    #[error(display = "failed to truncate firmware metadata file")]
    Truncate(#[error(cause)] io::Error),
}

/// Information about an available fwupd remote.
#[derive(Debug, Default)]
pub struct Remote {
    pub _type: u16,
    pub agreement: Option<Box<str>>,
    pub checksum: Option<Box<str>>,
    pub enabled: bool,
    pub filename_cache: Box<str>,
    pub filename_source: Box<str>,
    pub keyring: u16,
    pub modification_time: u64,
    pub priority: i16,
    pub remote_id: Box<str>,
    pub report_uri: Option<Box<str>>,
    pub title: Box<str>,
    pub uri: Option<Box<str>>,
}

impl Remote {
    pub fn update_metadata(
        &self,
        client: &Client,
        http_client: &reqwest::Client,
    ) -> Result<(), UpdateError> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(ref uri) = self.uri {
            if let Some(file) = self.update_file(http_client, uri)? {
                let sig = self.update_signature(http_client, uri)?;
                client
                    .update_metadata(&self.remote_id, file, sig)
                    .map_err(UpdateError::Client)?;
            }
        }

        Ok(())
    }

    fn update_file(
        &self,
        client: &reqwest::Client,
        uri: &str,
    ) -> Result<Option<File>, UpdateError> {
        let cache = Path::new(self.filename_cache.as_ref());

        if let Some(parent) = cache.parent() {
            fs::create_dir_all(parent).map_err(UpdateError::CreateParent)?;
        }

        let mut file = if cache.exists() && self.checksum.is_some() {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(cache)
                .map_err(|why| UpdateError::Open(why, cache.to_path_buf()))?;

            let checksum = self.checksum.as_ref().unwrap();
            let checksum_matched = crate::common::validate_checksum(
                &mut file,
                checksum,
                crate::common::checksum_guess_kind(checksum),
            )
            .map_err(|why| UpdateError::Read(why, cache.to_path_buf()))?;

            if checksum_matched {
                return Ok(None);
            }

            file.seek(SeekFrom::Start(0)).map_err(UpdateError::Seek)?;
            file.set_len(0).map_err(UpdateError::Truncate)?;

            file
        } else {
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(cache)
                .map_err(|why| UpdateError::Open(why, cache.to_path_buf()))?
        };

        client
            .get(uri)
            .send()
            .map_err(UpdateError::Get)?
            .error_for_status()
            .map_err(UpdateError::Get)?
            .copy_to(&mut file)
            .map_err(UpdateError::Copy)?;

        file.seek(SeekFrom::Start(0)).map_err(UpdateError::Seek)?;

        Ok(Some(file))
    }

    fn update_signature(&self, client: &reqwest::Client, uri: &str) -> Result<File, UpdateError> {
        let path = [self.filename_cache.as_ref(), ".asc"].concat();
        let cache = Path::new(path.as_str());

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(cache)
            .map_err(|why| UpdateError::Open(why, cache.to_path_buf()))?;

        client
            .get([uri.as_ref(), ".asc"].concat().as_str())
            .send()
            .map_err(UpdateError::Get)?
            .error_for_status()
            .map_err(UpdateError::Get)?
            .copy_to(&mut file)
            .map_err(UpdateError::Copy)?;

        file.seek(SeekFrom::Start(0)).map_err(UpdateError::Seek)?;

        Ok(file)
    }
}

impl FromIterator<DBusEntry> for Remote {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = DBusEntry>,
    {
        let mut remote = Remote::default();

        for (key, value) in iter {
            let key = key.as_str();
            match key {
                "Agreement" => remote.agreement = Some(dbus_str(&value, key).into()),
                "Checksum" => remote.checksum = Some(dbus_str(&value, key).into()),
                "Enabled" => remote.enabled = dbus_u64(&value, key) != 0,
                "FilenameCache" => remote.filename_cache = dbus_str(&value, key).into(),
                "FilenameSource" => remote.filename_source = dbus_str(&value, key).into(),
                "Keyring" => remote.keyring = dbus_u64(&value, key) as u16,
                "ModificationTime" => remote.modification_time = dbus_u64(&value, key),
                "Priority" => {
                    let value = value
                        .as_iter()
                        .expect("Priority is not a variant")
                        .next()
                        .expect("Priority does not contain a value");

                    remote.priority = dbus_i64(&value, key) as i16;
                }
                "RemoteId" => remote.remote_id = dbus_str(&value, key).into(),
                "ReportUri" => remote.report_uri = Some(dbus_str(&value, key).into()),
                "Title" => remote.title = dbus_str(&value, key).into(),
                "Type" => remote._type = dbus_u64(&value, key) as u16,
                "Uri" => remote.uri = Some(dbus_str(&value, key).into()),
                other => {
                    eprintln!("unknown remote key: {} ({})", other, value.signature());
                }
            }
        }

        remote
    }
}
