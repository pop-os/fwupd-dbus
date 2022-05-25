use crate::{common::*, dbus_helpers::*, Client, DBusEntry};
use dbus::arg::RefArg;
use std::{
    borrow::Cow,
    fs::{metadata, File, OpenOptions},
    io::{self, Seek, SeekFrom},
    iter::FromIterator,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use url::Url;

/// Describes the type of keyring to use with a remote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyringKind {
    Unknown,
    None,
    GPG,
    PKCS7,
    JCAT,
}

impl From<u8> for KeyringKind {
    fn from(value: u8) -> KeyringKind {
        use self::KeyringKind::*;
        match value {
            0 => Unknown,
            1 => None,
            2 => GPG,
            3 => PKCS7,
            4 => JCAT,
            _ => Unknown,
        }
    }
}

impl Default for KeyringKind {
    fn default() -> Self { KeyringKind::None }
}

/// Describes the kind of remote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteKind {
    Unknown,
    Download,
    Local,
    Directory,
}

impl From<u8> for RemoteKind {
    fn from(value: u8) -> RemoteKind {
        use self::RemoteKind::*;
        match value {
            1 => Download,
            2 => Local,
            3 => Directory,
            _ => Unknown,
        }
    }
}

impl Default for RemoteKind {
    fn default() -> Self { RemoteKind::Unknown }
}

/// An error that may occur when updating the metadata for a remote.
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("fwupd client errored when updating metadata for remote")]
    Client(#[source] crate::Error),
    #[error("failed to write firmware metadata to disk")]
    Copy(#[source] io::Error),
    #[error("failed to create parent directories for the remote's metadata cache")]
    CreateParent(#[source] io::Error),
    #[error("remote returned error when fetching firmware metadata")]
    Get(#[source] ureq::Error),
    #[error("attempted to update a remote without a URI")]
    NoUri,
    #[error("unable to open cached firmware metadata ({:?}) for remote", _1)]
    Open(#[source] io::Error, PathBuf),
    #[error("failed to read the cached firmware metadata ({:?}) for remote", _1)]
    Read(#[source] io::Error, PathBuf),
    #[error("failed to seek to beginning of firmware file")]
    Seek(#[source] io::Error),
    #[error("failed to truncate firmware metadata file")]
    Truncate(#[source] io::Error),
    #[error("failed to get fwupd user agent")]
    UserAgent(#[source] crate::Error),
}

/// The remote ID of a remote.
#[derive(Clone, Debug, Default, Eq, PartialEq, Shrinkwrap)]
pub struct RemoteId(pub(crate) Box<str>);

/// Information about an available fwupd remote.
#[derive(Clone, Debug, Default)]
pub struct Remote {
    pub agreement:         Option<Box<str>>,
    pub approval_required: bool,
    pub checksum:          Option<Box<str>>,
    pub enabled:           bool,
    pub filename_cache:    Box<str>,
    pub filename_source:   Box<str>,
    pub firmware_base_uri: Option<Box<str>>,
    pub keyring:           KeyringKind,
    pub kind:              RemoteKind,
    pub modification_time: u64,
    pub password:          Option<Box<str>>,
    pub priority:          i16,
    pub remote_id:         RemoteId,
    pub report_uri:        Option<Box<str>>,
    pub title:             Box<str>,
    pub uri:               Option<Box<str>>,
    pub username:          Option<Box<str>>,
}

impl Remote {
    /// Updates the metadata for this remote.
    pub fn update_metadata(&self, client: &Client) -> Result<(), UpdateError> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(ref uri) = self.uri {
            if let Some(file) = self.update_file(&client.http, uri)? {
                let sig = self.update_signature(&client.http, uri)?;
                client.update_metadata(&self, file, sig).map_err(UpdateError::Client)?;
            }
        }

        Ok(())
    }

    pub(crate) fn firmware_uri(&self, url: &str) -> Url {
        let uri = if let Some(ref firmware_base_uri) = self.firmware_base_uri {
            let mut firmware_base_uri: &str = firmware_base_uri;
            if firmware_base_uri.ends_with('/') {
                firmware_base_uri = &firmware_base_uri[..firmware_base_uri.len() - 1];
            }

            let basename = Path::new(url)
                .file_name()
                .expect("release URI without basename")
                .to_str()
                .expect("basename of release URI is not UTF-8");

            Cow::Owned([firmware_base_uri, "/", basename].concat())
        // Use the base URI of the metadata to build the full path.
        } else if !url.contains('/') {
            let remote_uri: &str = self.uri.as_ref().expect("remote URI without URI");
            let mut dirname = Path::new(remote_uri)
                .parent()
                .expect("metadata URI without parent")
                .as_os_str()
                .to_str()
                .expect("metadata URI is not UTF-8");

            if dirname.ends_with('/') {
                dirname = &dirname[..dirname.len() - 1];
            }

            Cow::Owned([dirname, "/", url].concat())
        // A normal URI
        } else {
            Cow::Borrowed(url)
        };

        uri.parse::<Url>().expect("firmware uri is not a valid uri")
    }

    /// Fetch the time since the last update, if such a time can be fetched.
    pub fn time_since_last_update(&self) -> Option<Duration> {
        metadata(&self.local_cache(self.filename_cache.as_ref()))
            .and_then(|md| md.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
    }

    fn local_cache(&self, file: &str) -> PathBuf {
        let file_name =
            Path::new(file).file_name().expect("remote filename cache does not have a file name");

        let id: &str = &*self.remote_id;
        cache_path(&Path::new(id).join(file_name))
    }

    /// Fetch the latest firmware from the remote
    fn update_file(&self, http: &ureq::Agent, uri: &str) -> Result<Option<File>, UpdateError> {
        let local_cache = &self.local_cache(self.filename_cache.as_ref());
        let checksum = self.checksum.as_ref().unwrap();

        if local_cache.exists() && self.checksum.is_some() {
            let checksum_matched = (|| {
                let mut file = OpenOptions::new().read(true).open(local_cache)?;

                validate_checksum(&mut file, checksum, checksum_guess_kind(checksum))
            })();

            if checksum_matched.is_ok() {
                return Ok(None);
            }
        };

        let file = Remote::fetch(http, uri, local_cache)?;

        Ok(Some(file))
    }

    /// Fetch the latest signature for the remote
    fn update_signature(&self, http: &ureq::Agent, uri: &str) -> Result<File, UpdateError> {
        let extension = match self.keyring {
            KeyringKind::JCAT => ".jcat",
            KeyringKind::PKCS7 => ".p7b",
            _ => ".asc",
        };

        let cache = &self.local_cache(&[self.filename_cache.as_ref(), extension].concat());
        let uri = [uri, extension].concat();

        Remote::fetch(http, &uri, cache)
    }

    /// Fetch a file from a remote URI to disk
    fn fetch(http: &ureq::Agent, uri: &str, file: &Path) -> Result<File, UpdateError> {
        info!("fetching {} to {:?}", uri, file);

        if file.exists() {
            let _ = std::fs::remove_file(file);
        }

        // Open the file that we're going to write to
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file)
            .map_err(|why| UpdateError::Open(why, file.to_path_buf()))?;

        // Initiate connection to fetch firmware from remote
        let mut resp = http.get(uri).call().map_err(UpdateError::Get)?.into_reader();

        std::io::copy(&mut resp, &mut file).map_err(UpdateError::Copy)?;

        file.seek(SeekFrom::Start(0)).map_err(UpdateError::Seek)?;

        Ok(file)
    }
}

impl AsRef<RemoteId> for Remote {
    fn as_ref(&self) -> &RemoteId { &self.remote_id }
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
                "ApprovalRequired" => remote.approval_required = dbus_u64(&value, key) != 0,
                KEY_CHECKSUM => remote.checksum = Some(dbus_str(&value, key).into()),
                "Enabled" => remote.enabled = dbus_u64(&value, key) != 0,
                "FilenameCache" => remote.filename_cache = dbus_str(&value, key).into(),
                "FilenameSource" => remote.filename_source = dbus_str(&value, key).into(),
                "FirmwareBaseUri" => remote.firmware_base_uri = Some(dbus_str(&value, key).into()),
                "Keyring" => remote.keyring = KeyringKind::from(dbus_u64(&value, key) as u8),
                "ModificationTime" => remote.modification_time = dbus_u64(&value, key),
                "Password" => remote.password = Some(dbus_str(&value, key).into()),
                "Priority" => {
                    let value = value
                        .as_iter()
                        .expect("Priority is not a variant")
                        .next()
                        .expect("Priority does not contain a value");

                    remote.priority = dbus_i64(&value, key) as i16;
                }
                KEY_REMOTE_ID => remote.remote_id = RemoteId(dbus_str(&value, key).into()),
                "ReportUri" => remote.report_uri = Some(dbus_str(&value, key).into()),
                "Title" => remote.title = dbus_str(&value, key).into(),
                "Type" => remote.kind = RemoteKind::from(dbus_u64(&value, key) as u8),
                "Username" => remote.username = Some(dbus_str(&value, key).into()),
                KEY_URI => remote.uri = Some(dbus_str(&value, key).into()),
                other => {
                    eprintln!("unknown remote key: {} ({}): {:?}", other, value.signature(), value);
                }
            }
        }

        remote
    }
}
