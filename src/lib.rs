#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cascade;
#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate log;
#[macro_use]
extern crate shrinkwraprs;

mod common;
mod dbus_helpers;
mod device;
mod release;
mod remote;
pub mod request;

pub use self::{device::*, release::*, remote::*};

use base64::write::EncoderWriter as Base64Encoder;
use dbus::{
    self,
    arg::{Arg, Array, Dict, Get, OwnedFd, RefArg, Variant},
    ffidisp::{
        stdintf::org_freedesktop_dbus::{Peer, Properties},
        ConnPath, Connection,
    },
    Message,
};
use request::Request;
use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    iter::FromIterator,
    os::unix::io::IntoRawFd,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use zbus::zvariant::Value;

pub const DBUS_NAME: &str = "org.freedesktop.fwupd";
pub const DBUS_IFACE: &str = "org.freedesktop.fwupd";
pub const DBUS_PATH: &str = "/";

const TIMEOUT: i32 = -1;

pub type DynVariant = Variant<Box<dyn RefArg + 'static>>;
pub type DBusEntry = (String, DynVariant);

bitflags! {
    /// Controls the behavior of the install method.
    pub struct InstallFlags: u16 {
        const OFFLINE             = 1;
        const ALLOW_REINSTALL     = 1 << 1;
        const ALLOW_OLDER         = 1 << 2;
        const FORCE               = 1 << 3;
        const NO_HISTORY          = 1 << 4;
        const ALLOW_BRANCH_SWITCH = 1 << 5;
        const IGNORE_CHECKSUM     = 1 << 6;
        const IGNORE_VID_PID      = 1 << 7;
        const IGNORE_POWER        = 1 << 8;
        const NO_SEARCH           = 1 << 9;
    }
}

bitflags! {
    /// Sets what features are supported by the client
    pub struct FeatureFlags: u64 {
        const CAN_REPORT = 1;
        const DETACH_ACTION = 1 << 1;
        const UPDATE_ACTION = 1 << 2;
        const SWITCH_BRANCH = 1 << 3;
        const REQUESTS = 1 << 4;
        const FDE_WARNING = 1 << 5;
        const COMMUNITY_TEXT = 1 << 6;
    }
}

/// Describes the status of the daemon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    Unknown,
    Idle,
    Loading,
    Decompressing,
    DeviceRestart,
    DeviceWrite,
    Scheduling,
    Downloading,
    DeviceRead,
    DeviceErase,
    WaitingForAuth,
    DeviceBusy,
    Shutdown,
}

impl From<u8> for Status {
    fn from(value: u8) -> Self {
        use self::Status::*;
        match value {
            0 => Unknown,
            1 => Idle,
            2 => Loading,
            3 => Decompressing,
            4 => DeviceRestart,
            5 => DeviceWrite,
            6 => Scheduling,
            7 => Downloading,
            8 => DeviceRead,
            9 => DeviceErase,
            10 => WaitingForAuth,
            11 => DeviceBusy,
            12 => Shutdown,
            _ => {
                eprintln!("status value {} is out of range", value);
                Idle
            }
        }
    }
}

#[derive(Debug)]
pub enum FlashEvent {
    DownloadInitiate(u64),
    DownloadUpdate(usize),
    DownloadComplete,
    VerifyingChecksum,
    FlashInProgress,
}

/// An error that may occur when using the client.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to add match on client connection")]
    AddMatch(#[source] dbus::Error),
    #[error("argument mismatch in {} method", _0)]
    ArgumentMismatch(&'static str, #[source] dbus::arg::TypeMismatchError),
    #[error("calling {} method failed", _0)]
    Call(&'static str, #[source] dbus::Error),
    #[error("unable to establish dbus connection")]
    Connection(#[source] dbus::Error),
    #[error("the remote firmware which was downloaded has an invalid checksum")]
    FirmwareChecksumMismatch,
    #[error("failed to copy firmware file from remote")]
    FirmwareCopy(#[source] io::Error),
    #[error("failed to create firmware file in user cache")]
    FirmwareCreate(#[source] io::Error),
    #[error("failed to GET firmware file from remote")]
    FirmwareGet(#[source] ureq::Error),
    #[error("failed to open firmware file")]
    FirmwareOpen(#[source] io::Error),
    #[error("failed to read firmware file")]
    FirmwareRead(#[source] io::Error),
    #[error("failed to seek to beginning of firmware file")]
    FirmwareSeek(#[source] io::Error),
    #[error("failed to get property for {}", _0)]
    GetProperty(&'static str, #[source] dbus::Error),
    #[error("unable to ping the dbus daemon")]
    Ping(#[source] dbus::Error),
    #[error("failed to create {} method call", _0)]
    NewMethodCall(&'static str, String),
    #[error("release does not have any checksums to validate firmware with")]
    ReleaseWithoutChecksums,
    #[error("remote not found")]
    RemoteNotFound,
}

/// A DBus client for interacting with the fwupd daemon.
pub struct Client {
    connection: Connection,

    pub client_name: String,

    http: ureq::Agent,
}

impl Client {
    pub fn new() -> Result<Self, Error> {
        let connection = Connection::new_system().map_err(Error::Connection)?;

        let mut client = Self { connection, client_name: String::new(), http: ureq::Agent::new() };

        // Reassign the user agent of our client
        client.client_name = ["fwupd/", &*client.daemon_version()?].concat();

        client.http = ureq::AgentBuilder::new().user_agent(client.client_name.as_str()).build();

        Ok(client)
    }

    /// Activate a firmware update on the device.
    pub fn activate<D: AsRef<DeviceId>>(&self, id: D) -> Result<(), Error> {
        self.action_method("Activate", id.as_ref().as_ref())
    }

    /// Clears the results of an offline update.
    pub fn clear_results<D: AsRef<DeviceId>>(&self, id: D) -> Result<(), Error> {
        self.action_method("ClearResults", id.as_ref().as_ref())
    }

    /// The version of this daemon.
    pub fn daemon_version(&self) -> Result<Box<str>, Error> {
        self.get_property::<String>("DaemonVersion").map(Box::from)
    }

    /// Gets details about a local firmware file.
    pub fn details<H: IntoRawFd>(
        &self,
        handle: H,
    ) -> Result<Vec<HashMap<String, DynVariant>>, Error> {
        self.get_handle_method("GetDetails", handle)
    }

    /// Gets a list of all the devices that are supported.
    pub fn devices(&self) -> Result<Vec<Device>, Error> { self.get_method("GetDevices") }

    /// Get a list of all the downgrades possible for a specific device.
    pub fn downgrades<D: AsRef<DeviceId>>(&self, device_id: D) -> Result<Vec<Release>, Error> {
        self.get_device_method("GetDowngrades", device_id.as_ref().as_ref())
    }

    /// Fetches firmware from a remote and caches it for later use.
    ///
    /// Firmware will only be fetched if it has not already been cached, or the cached firmware has
    /// an invalid checksum.
    pub fn fetch_firmware_from_release<C: FnMut(FlashEvent)>(
        &self,
        device: &Device,
        release: &Release,
        mut callback: Option<C>,
    ) -> Result<(PathBuf, Option<File>), Error> {
        let remote = self.remote(release)?;

        // If remote is local, we already have the firmware.
        {
            let filename: Option<Cow<'_, Path>> = match remote.kind {
                RemoteKind::Local => Some(Cow::Owned(
                    Path::new(remote.filename_cache.as_ref())
                        .parent()
                        .expect("remote filename cache without parent")
                        .join(Path::new(release.uri.as_ref())),
                )),
                RemoteKind::Directory => Some(Cow::Borrowed(Path::new(&release.uri[7..]))),
                _ => None,
            };

            if let Some(filename) = filename {
                return Ok((filename.to_path_buf(), None));
            }
        }

        // Create URI, substituting if required.
        let uri = remote.firmware_uri(&release.uri);
        let file_path = common::cache_path_from_uri(&uri);

        let mut request = self.http.get(uri.to_string().as_str());

        // Set the username and password.
        if let Some(ref username) = remote.username {
            let password = remote.password.as_ref();

            // Basic HTTP Auth
            let mut header_value = b"Basic ".to_vec();

            {
                let mut encoder = Base64Encoder::new(&mut header_value, base64::STANDARD);
                write!(encoder, "{}:", username).unwrap();
                if let Some(password) = password {
                    write!(encoder, "{}", password).unwrap();
                }
            }

            if let Ok(value) = String::from_utf8(header_value) {
                request = request.set("Authorization", &value);
            }
        }

        let (checksum, algorithm) =
            common::find_best_checksum(&release.checksums).ok_or(Error::ReleaseWithoutChecksums)?;

        // Closure for downloading the firmware to our file, and then validating that it is correct.
        let download_and_verify = |mut file: File| {
            info!("downloading firmware for {} ({})...", device.name, release.version);
            if let Some(ref mut cb) = callback {
                cb(FlashEvent::DownloadInitiate(release.size));
            }

            let mut response = request.call().map_err(Error::FirmwareGet)?.into_reader();

            match callback {
                Some(ref mut callback) => {
                    let result = (|| {
                        let mut progress = 0;
                        let mut buffer = vec![0u8; 8192];

                        loop {
                            let read = response.read(&mut buffer[..])?;
                            if read == 0 {
                                break;
                            }
                            file.write_all(&buffer[..read])?;
                            progress += read;
                            callback(FlashEvent::DownloadUpdate(progress))
                        }

                        Ok(file)
                    })();

                    callback(FlashEvent::DownloadComplete);
                    file = result.map_err(Error::FirmwareCopy)?;
                }
                None => {
                    io::copy(&mut response, &mut file).map_err(Error::FirmwareCopy)?;
                }
            };

            file.seek(SeekFrom::Start(0)).map_err(Error::FirmwareSeek)?;

            if let Some(ref mut cb) = callback {
                cb(FlashEvent::VerifyingChecksum);
            }

            info!("validating firmware for {} ({})", device.name, release.version);
            let checksum_matched = common::validate_checksum(&mut file, checksum, algorithm);

            if checksum_matched.is_err() {
                return Err(Error::FirmwareChecksumMismatch);
            }

            Ok(file)
        };

        let mut file = None;

        // If the firmware does not exist, or the checksum is invalid, it will need to be fetched.
        let firmware_requires_fetching = if file_path.exists() {
            info!("validating firmware for {} ({})", device.name, release.version);
            let mut cache =
                OpenOptions::new().read(true).open(&file_path).map_err(Error::FirmwareOpen)?;

            let result = common::validate_checksum(&mut cache, checksum, algorithm).is_err();

            file = Some(cache);
            result
        } else {
            true
        };

        if firmware_requires_fetching {
            let download = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&file_path)
                .map_err(Error::FirmwareCreate)?;

            // If any error occurs when downloading or verifying, delete the file that we created.
            let download = match download_and_verify(download) {
                Ok(download) => download,
                Err(why) => {
                    let _ = fs::remove_file(&file_path);
                    return Err(why);
                }
            };

            file = Some(download);
        }

        if let Some(ref mut file) = file {
            file.seek(SeekFrom::Start(0)).map_err(Error::FirmwareSeek)?;
        }

        Ok((file_path, file))
    }

    /// Update firmware for a `Device` with the firmware specified in a `Release`.
    pub fn update_device_with_release<F: FnMut(FlashEvent)>(
        &self,
        device: &Device,
        release: &Release,
        mut flags: InstallFlags,
        mut callback: Option<F>,
    ) -> Result<(), Error> {
        if device.only_offline() {
            flags |= InstallFlags::OFFLINE;
        }

        let (filename, file) =
            self.fetch_firmware_from_release(device, release, callback.as_mut())?;

        if let Some(ref mut cb) = callback {
            cb(FlashEvent::FlashInProgress);
        }

        info!("installing firmware for {} ({})", device.name, release.version);
        self.install(device, "(user)", &filename, file, flags)
    }

    /// Gets a list of all the past firmware updates.
    pub fn history<H: IntoRawFd>(&self, handle: H) -> Result<Vec<Device>, Error> {
        self.get_handle_method("GetHistory", handle)
    }

    /// Schedules a firmware to be installed.
    pub fn install<D: AsRef<DeviceId>, H: IntoRawFd>(
        &self,
        id: D,
        reason: &str,
        filename: &Path,
        handle: Option<H>,
        flags: InstallFlags,
    ) -> Result<(), Error> {
        const METHOD: &str = "Install";

        let fd = match handle {
            Some(handle) => handle.into_raw_fd(),
            None => OpenOptions::new()
                .read(true)
                .open(filename)
                .map_err(Error::FirmwareOpen)?
                .into_raw_fd(),
        };

        let filename = filename.as_os_str().to_str().expect("filename is not UTF-8");

        let mut options: HashMap<&str, DynVariant> = cascade! {
            HashMap::new();
            ..insert("reason", Variant(Box::new(reason.to_owned()) as Box<dyn RefArg>));
            ..insert("filename", Variant(Box::new(filename.to_owned()) as Box<dyn RefArg>));
        };

        fn boolean_variant(value: bool) -> Variant<Box<dyn RefArg>> {
            Variant(Box::new(value) as Box<dyn RefArg>)
        }

        if flags.contains(InstallFlags::OFFLINE) {
            options.insert("offline", boolean_variant(true));
        }

        if flags.contains(InstallFlags::ALLOW_OLDER) {
            options.insert("allow-older", boolean_variant(true));
        }

        if flags.contains(InstallFlags::ALLOW_REINSTALL) {
            options.insert("allow-reinstall", boolean_variant(true));
        }

        if flags.contains(InstallFlags::ALLOW_BRANCH_SWITCH) {
            options.insert("allow-branch-switch", boolean_variant(true));
        }

        if flags.contains(InstallFlags::FORCE) {
            options.insert("force", boolean_variant(true));
        }

        if flags.contains(InstallFlags::IGNORE_POWER) {
            options.insert("ignore-power", boolean_variant(true));
        }

        if flags.contains(InstallFlags::NO_HISTORY) {
            options.insert("no-history", boolean_variant(true));
        }

        let id: &str = id.as_ref().as_ref();
        let cb = |m: Message| m.append3(id, unsafe { OwnedFd::new(fd) }, options);

        self.call_method(METHOD, cb)?;
        Ok(())
    }

    /// Listens for signals from the DBus daemon.
    pub fn listen_signals(
        &self,
        cancellable: Arc<AtomicBool>,
    ) -> zbus::Result<impl Iterator<Item = Signal> + '_> {
        let connection = zbus::blocking::Connection::system()?;

        let proxy = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.fwupd",
            "/",
            "org.freedesktop.fwupd",
        )?;

        Ok(proxy
            .receive_all_signals()?
            .take_while(move |_| cancellable.load(Ordering::SeqCst))
            .filter_map(|signal| {
                let signal: zbus::Result<Signal> = match &*signal.member().unwrap() {
                    "DeviceRequest" => signal.body().map(|array: HashMap<String, Value>| {
                        let mut request = request::Request::default();
                        for (key, value) in array {
                            match key.as_str() {
                                "AppstreamId" => {
                                    if let Value::Str(value) = value {
                                        request.appstream_id = value.as_str().to_owned();
                                    }
                                }

                                "Created" => {
                                    if let Value::U64(value) = value {
                                        request.created = value;
                                    }
                                }

                                "Plugin" => {
                                    if let Value::Str(value) = value {
                                        request.plugin = value.as_str().to_owned();
                                    }
                                }

                                "RequestKind" => {
                                    if let Value::U32(value) = value {
                                        request.request_kind = value;
                                    }
                                }

                                "UpdateMessage" => {
                                    if let Value::Str(value) = value {
                                        request.update_message = value.as_str().to_owned();
                                    }
                                }

                                _ => {
                                    warn!("unknown DeviceRequest field: {}", key);
                                }
                            }
                        }
                        Signal::DeviceRequest(request::Request::default())
                    }),
                    _ => return None,
                };

                match signal {
                    Ok(signal) => Some(signal),
                    Err(why) => {
                        eprintln!("signal error: {}", why);
                        None
                    }
                }
            }))
    }

    /// Modifies a device in some way.
    pub fn modify_device<D: AsRef<DeviceId>>(
        &self,
        device_id: D,
        key: &str,
        value: &str,
    ) -> Result<(), Error> {
        let device_id: &str = device_id.as_ref().as_ref();
        self.call_method("ModifyDevice", |m| m.append3(device_id, key, value))?;
        Ok(())
    }

    /// Modifies a remote in some way.
    pub fn modify_remote<R: AsRef<RemoteId>>(
        &self,
        remote_id: R,
        key: &str,
        value: &str,
    ) -> Result<(), Error> {
        let remote_id: &str = remote_id.as_ref().as_ref();
        self.call_method("ModifyRemote", |m| m.append3(remote_id, key, value))?;
        Ok(())
    }

    /// The job percentage completion, or 0 for unknown.
    pub fn percentage(&self) -> Result<u8, Error> {
        self.get_property::<u32>("Percentage").map(|v| v as u8)
    }

    pub fn ping(&self) -> Result<(), Error> { self.connection_path().ping().map_err(Error::Ping) }

    /// Gets a list of all the releases for a specific device.
    pub fn releases<D: AsRef<DeviceId>>(&self, device_id: D) -> Result<Vec<Release>, Error> {
        self.get_device_method("GetReleases", device_id.as_ref().as_ref())
    }

    /// Find the remote with the given ID.
    pub fn remote<D: AsRef<RemoteId>>(&self, id: D) -> Result<Remote, Error> {
        self.remotes()?
            .into_iter()
            .find(|remote| &remote.remote_id == id.as_ref())
            .ok_or(Error::RemoteNotFound)
    }

    /// Gets the list of remotes.
    pub fn remotes(&self) -> Result<Vec<Remote>, Error> { self.get_method("GetRemotes") }

    /// Gets the results of an offline update.
    pub fn results<D: AsRef<DeviceId>>(&self, id: D) -> Result<Option<Device>, Error> {
        let id: &str = id.as_ref().as_ref();
        let message = self.call_method("GetResults", |m| m.append1(id))?;
        let iter: Option<Dict<String, Variant<Box<dyn RefArg + 'static>>, _>> = message.get1();
        Ok(iter.map(Device::from_iter))
    }

    /// Instructs the daemon about which features this client supports.
    pub fn set_feature_flags(&self, feature_flags: FeatureFlags) -> Result<(), Error> {
        self.call_method("SetFeatureFlags", |m| m.append1(feature_flags.bits()))?;
        Ok(())
    }

    /// The daemon status, e.g. `Decompressing`.
    pub fn status(&self) -> Result<Status, Error> {
        self.get_property::<u32>("Status").map(|v| Status::from(v as u8))
    }

    /// If the daemon has been tainted with a third party plugin.
    pub fn tainted(&self) -> Result<bool, Error> { self.get_property::<bool>("Tainted") }

    /// Unlock the device to allow firmware access.
    pub fn unlock<D: AsRef<DeviceId>>(&self, id: D) -> Result<(), Error> {
        self.action_method("Unlock", id.as_ref().as_ref())
    }

    /// Adds AppStream resource information from a session client.
    pub fn update_metadata<D: IntoRawFd, S: IntoRawFd, R: AsRef<RemoteId>>(
        &self,
        remote_id: R,
        data: D,
        signature: S,
    ) -> Result<(), Error> {
        let remote_id: &str = remote_id.as_ref().as_ref();
        let cb = |m: Message| {
            m.append3(remote_id, unsafe { OwnedFd::new(data.into_raw_fd()) }, unsafe {
                OwnedFd::new(signature.into_raw_fd())
            })
        };

        self.call_method("UpdateMetadata", cb)?;
        Ok(())
    }

    /// Get a list of all the upgrades possible for a specific device.
    pub fn upgrades<D: AsRef<DeviceId>>(&self, device_id: D) -> Result<Vec<Release>, Error> {
        self.get_device_method("GetUpgrades", device_id.as_ref().as_ref())
    }

    /// Verifies firmware on a device by reading it back and performing
    /// a cryptographic hash, typically SHA1.
    pub fn verify<D: AsRef<DeviceId>>(&self, id: D) -> Result<(), Error> {
        self.action_method("Verify", id.as_ref().as_ref())
    }

    /// Updates the cryptographic hash stored for a device.
    pub fn verify_update<D: AsRef<DeviceId>>(&self, id: D) -> Result<(), Error> {
        self.action_method("VerifyUpdate", id.as_ref().as_ref())
    }

    fn action_method(&self, method: &'static str, id: &str) -> Result<(), Error> {
        self.call_method(method, |m| m.append1(id))?;
        Ok(())
    }

    fn get_method<T: FromIterator<DBusEntry>>(
        &self,
        method: &'static str,
    ) -> Result<Vec<T>, Error> {
        let message = self.call_method(method, |m| m)?;
        let iter: Array<Dict<String, Variant<Box<dyn RefArg + 'static>>, _>, _> =
            message.read1().map_err(|why| Error::ArgumentMismatch(method, why))?;

        Ok(iter.map(T::from_iter).collect())
    }

    fn get_device_method<T: FromIterator<DBusEntry>, C: FromIterator<T>>(
        &self,
        method: &'static str,
        device_id: &str,
    ) -> Result<C, Error> {
        let message = self.call_method(method, |m| m.append1(device_id))?;
        let iter: Array<Dict<String, Variant<Box<dyn RefArg + 'static>>, _>, _> =
            message.read1().map_err(|why| Error::ArgumentMismatch(method, why))?;

        Ok(C::from_iter(iter.map(T::from_iter)))
    }

    fn get_handle_method<T: FromIterator<DBusEntry>, H: IntoRawFd>(
        &self,
        method: &'static str,
        handle: H,
    ) -> Result<Vec<T>, Error> {
        let cb = move |m: Message| m.append1(unsafe { OwnedFd::new(handle.into_raw_fd()) });

        let message = self.call_method(method, cb)?;
        let iter: Array<Dict<String, Variant<Box<dyn RefArg + 'static>>, _>, _> =
            message.read1().map_err(|why| Error::ArgumentMismatch(method, why))?;

        Ok(iter.map(T::from_iter).collect())
    }

    fn get_property<T: for<'a> Get<'a> + Arg>(&self, property: &'static str) -> Result<T, Error> {
        self.connection_path()
            .get::<T>(DBUS_NAME, property)
            .map_err(|why| Error::GetProperty(property, why))
    }

    fn call_method<F: FnOnce(Message) -> Message>(
        &self,
        method: &'static str,
        append_args: F,
    ) -> Result<Message, Error> {
        let mut m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, method)
            .map_err(|why| Error::NewMethodCall(method, why))?;

        m = append_args(m);

        self.connection
            .send_with_reply_and_block(m, TIMEOUT)
            .map_err(|why| Error::Call(method, why))
    }

    fn connection_path(&self) -> ConnPath<&Connection> {
        self.connection.with_path(DBUS_NAME, DBUS_PATH, TIMEOUT)
    }
}

/// Signal received by the daemon when listening for signal events with `Client::listen_signals()`.
#[derive(Debug)]
pub enum Signal {
    /// Some value on the interface or the number of devices or profiles has changed.
    Changed,
    /// A device has been added.
    DeviceAdded(Device),
    /// A device has been changed.
    DeviceChanged(Device),
    /// A device has been removed.
    DeviceRemoved(Device),
    /// Request for user interaction
    DeviceRequest(Request),
    /// Triggers when a property has changed.
    PropertiesChanged {
        interface:   Box<str>,
        changed:     HashMap<String, DynVariant>,
        invalidated: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn download_remote() -> Remote {
        Remote {
            enabled: true,
            kind: RemoteKind::Download,
            keyring: KeyringKind::GPG,
            firmware_base_uri: Some("https://my.fancy.cdn/".into()),
            uri: Some("https://s3.amazonaws.com/lvfsbucket/downloads/firmware.xml.gz".into()),
            ..Default::default()
        }
    }

    fn nopath_remote() -> Remote {
        Remote {
            enabled: true,
            kind: RemoteKind::Download,
            keyring: KeyringKind::GPG,
            uri: Some("https://s3.amazonaws.com/lvfsbucket/downloads/firmware.xml.gz".into()),
            ..Default::default()
        }
    }

    #[test]
    fn remote_baseuri() {
        let remote = download_remote();
        let firmware_uri = remote.firmware_uri("http://bbc.co.uk/firmware.cab");
        assert_eq!(firmware_uri.to_string().as_str(), "https://my.fancy.cdn/firmware.cab")
    }

    #[test]
    fn remote_nopath() {
        let remote = nopath_remote();
        let firmware_uri = remote.firmware_uri("firmware.cab");
        assert_eq!(
            firmware_uri.to_string().as_str(),
            "https://s3.amazonaws.com/lvfsbucket/downloads/firmware.cab"
        )
    }
}
