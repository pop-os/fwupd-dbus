#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cascade;
#[macro_use]
extern crate err_derive;
#[macro_use]
extern crate shrinkwraprs;

mod common;
mod dbus_helpers;
mod device;
mod release;
mod remote;

pub use self::device::*;
pub use self::release::*;
pub use self::remote::*;

use dbus::{
    self,
    arg::{Array, Dict, RefArg, Variant},
    BusType, Connection, ConnectionItem, Message, OwnedFd,
};

use std::{
    collections::HashMap,
    iter::FromIterator,
    os::unix::io::IntoRawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub const DBUS_NAME: &str = "org.freedesktop.fwupd";
pub const DBUS_IFACE: &str = "org.freedesktop.fwupd";
pub const DBUS_PATH: &str = "/";

const TIMEOUT: i32 = 5000;

pub type DynVariant = Variant<Box<RefArg + 'static>>;
pub type DBusEntry = (String, DynVariant);

bitflags! {
    pub struct InstallFlags: u64 {
        const OFFLINE         = 1 << 0;
        const ALLOW_REINSTALL = 1 << 1;
        const ALLOW_OLDER     = 1 << 2;
        const FORCE           = 1 << 3;
        const NO_HISTORY      = 1 << 4;
    }
}

/// An error that may occur when using the client.
#[derive(Debug, Error)]
pub enum Error {
    #[error(display = "failed to add match on client connection")]
    AddMatch(#[error(cause)] dbus::Error),
    #[error(display = "argument mismatch in {} method", _0)]
    ArgumentMismatch(&'static str, #[error(cause)] dbus::arg::TypeMismatchError),
    #[error(display = "calling {} method failed", _0)]
    Call(&'static str, #[error(cause)] dbus::Error),
    #[error(display = "unable to establish dbus connection")]
    Connection(#[error(cause)] dbus::Error),
    #[error(display = "failed to create {} method call", _0)]
    NewMethodCall(&'static str, String),
}

/// A DBus client for interacting with the fwupd daemon.
#[derive(Shrinkwrap)]
pub struct Client(dbus::Connection);

impl Client {
    pub fn new() -> Result<Self, Error> {
        Connection::get_private(BusType::System)
            .map(Self)
            .map_err(Error::Connection)
    }

    /// Activate a firmware update on the device.
    pub fn activate(&self, id: &str) -> Result<(), Error> {
        self.action_method("Activate", id)
    }

    /// Clears the results of an offline update.
    pub fn clear_results(&self, id: &str) -> Result<(), Error> {
        self.action_method("ClearResults", id)
    }

    /// Gets details about a local firmware file.
    pub fn get_details<H: IntoRawFd>(
        &self,
        handle: H,
    ) -> Result<Vec<HashMap<String, DynVariant>>, Error> {
        self.get_handle_method("GetDetails", handle)
    }

    /// Gets a list of all the devices that are supported.
    pub fn get_devices(&self) -> Result<Vec<Device>, Error> {
        self.get_method("GetDevices")
    }

    /// Get a list of all the downgrades possible for a specific device.
    pub fn get_downgrades(&self, device_id: &str) -> Result<Vec<Release>, Error> {
        self.get_device_method("GetDowngrades", device_id)
    }

    /// Gets a list of all the past firmware updates.
    pub fn get_history<H: IntoRawFd>(
        &self,
        handle: H,
    ) -> Result<Vec<HashMap<String, DynVariant>>, Error> {
        self.get_handle_method("GetHistory", handle)
    }

    /// Gets a list of all the releases for a specific device.
    pub fn get_releases(&self, device_id: &str) -> Result<Vec<Release>, Error> {
        self.get_device_method("GetReleases", device_id)
    }

    /// Gets the list of remotes.
    pub fn get_remotes(&self) -> Result<Vec<Remote>, Error> {
        self.get_method("GetRemotes")
    }

    /// Gets the results of an offline update.
    pub fn get_results(&self, id: &str) -> Result<HashMap<String, DynVariant>, Error> {
        const METHOD: &str = "GetResults";

        self.call_method(METHOD, |m| m.append1(id))?
            .read1()
            .map_err(|why| Error::ArgumentMismatch(METHOD, why))
    }

    /// Get a list of all the upgrades possible for a specific device.
    pub fn get_upgrades(&self, device_id: &str) -> Result<Vec<Release>, Error> {
        self.get_device_method("GetUpgrades", device_id)
    }

    /// Schedules a firmware to be installed.
    pub fn install<'a, H: IntoRawFd>(
        &self,
        id: &str,
        reason: &str,
        filename: &str,
        handle: H,
        flags: InstallFlags,
    ) -> Result<HashMap<String, DynVariant>, Error> {
        const METHOD: &str = "Install";

        let options: Vec<(&str, DynVariant)> = cascade! {
            opts: Vec::with_capacity(8);
            ..push(("reason", Variant(Box::new(reason.to_owned()) as Box<dyn RefArg>)));
            ..push(("filename", Variant(Box::new(filename.to_owned()) as Box<dyn RefArg>)));
            | if flags.contains(InstallFlags::OFFLINE) {
                opts.push(("offline", Variant(Box::new(true) as Box<dyn RefArg>)));
            };
            | if flags.contains(InstallFlags::ALLOW_OLDER) {
                opts.push(("allow-older", Variant(Box::new(true) as Box<dyn RefArg>)));
            };
            | if flags.contains(InstallFlags::ALLOW_REINSTALL) {
                opts.push(("allow-reinstall", Variant(Box::new(true) as Box<dyn RefArg>)));
            };
            | if flags.contains(InstallFlags::FORCE) {
                opts.push(("force", Variant(Box::new(true) as Box<dyn RefArg>)));
            };
            | if flags.contains(InstallFlags::NO_HISTORY) {
                opts.push(("no-history", Variant(Box::new(true) as Box<dyn RefArg>)));
            };
        };

        let options = Array::new(options);

        let cb = |m: Message| m.append3(id, OwnedFd::new(handle.into_raw_fd()), options);

        self.call_method(METHOD, cb)?
            .read1()
            .map_err(|why| Error::ArgumentMismatch(METHOD, why))
    }

    /// Listens for signals from the DBus daemon.
    pub fn listen_signals<'a>(
        &'a self,
        cancellable: Arc<AtomicBool>,
    ) -> impl Iterator<Item = Signal> + 'a {
        fn filter_signal(ci: ConnectionItem) -> Option<Message> {
            if let ConnectionItem::Signal(ci) = ci {
                Some(ci)
            } else {
                None
            }
        }

        fn read_signal<T: FromIterator<DBusEntry>>(
            signal: Message,
            method: &'static str,
        ) -> Result<T, Error> {
            let iter: Dict<String, Variant<Box<RefArg + 'static>>, _> = signal
                .read1()
                .map_err(|why| Error::ArgumentMismatch(method, why))?;

            Ok(T::from_iter(iter))
        }

        self.iter(TIMEOUT)
            .take_while(move |_| cancellable.load(Ordering::SeqCst))
            .filter_map(filter_signal)
            .filter_map(|signal| {
                let signal = match &*signal.member().unwrap() {
                    "Changed" => Ok(Signal::Changed),
                    "DeviceAdded" => read_signal(signal, "DeviceAdded").map(Signal::DeviceAdded),
                    "DeviceChanged" => {
                        read_signal(signal, "DeviceChanged").map(Signal::DeviceChanged)
                    }
                    "DeviceRemoved" => {
                        read_signal(signal, "DeviceRemoved").map(Signal::DeviceRemoved)
                    }
                    _ => return None,
                };

                match signal {
                    Ok(signal) => Some(signal),
                    Err(why) => {
                        eprintln!("signal error: {}", why);
                        None
                    }
                }
            })
    }

    /// Modifies a device in some way.
    pub fn modify_device(&self, device_id: &str, key: &str, value: &str) -> Result<(), Error> {
        self.call_method("ModifyDevice", |m| m.append3(device_id, key, value))?;
        Ok(())
    }

    /// Modifies a remote in some way.
    pub fn modify_remote(&self, remote_id: &str, key: &str, value: &str) -> Result<(), Error> {
        self.call_method("ModifyRemote", |m| m.append3(remote_id, key, value))?;
        Ok(())
    }

    /// Unlock the device to allow firmware access.
    pub fn unlock(&self, id: &str) -> Result<(), Error> {
        self.action_method("Unlock", id)
    }

    /// Adds AppStream resource information from a session client.
    pub fn update_metadata<D: IntoRawFd, S: IntoRawFd>(
        &self,
        remote_id: &str,
        data: D,
        signature: S,
    ) -> Result<(), Error> {
        let cb = |m: Message| {
            m.append3(
                remote_id,
                OwnedFd::new(data.into_raw_fd()),
                OwnedFd::new(signature.into_raw_fd()),
            )
        };

        self.call_method("UpdateMetadata", cb)?;
        Ok(())
    }

    /// Verifies firmware on a device by reading it back and performing
    /// a cryptographic hash, typically SHA1.
    pub fn verify(&self, id: &str) -> Result<(), Error> {
        self.action_method("Verify", id)
    }

    /// Updates the cryptographic hash stored for a device.
    pub fn verify_update(&self, id: &str) -> Result<(), Error> {
        self.action_method("VerifyUpdate", id)
    }

    fn action_method(&self, method: &'static str, id: &str) -> Result<(), Error> {
        self.call_method(method, |m| m.append(id))?;
        Ok(())
    }

    fn get_method<T: FromIterator<DBusEntry>>(
        &self,
        method: &'static str,
    ) -> Result<Vec<T>, Error> {
        let message = self.call_method(method, |m| m)?;
        let iter: Array<Dict<String, Variant<Box<RefArg + 'static>>, _>, _> = message
            .read1()
            .map_err(|why| Error::ArgumentMismatch(method, why))?;

        Ok(iter.map(T::from_iter).collect())
    }

    fn get_device_method<T: FromIterator<DBusEntry>>(
        &self,
        method: &'static str,
        device_id: &str,
    ) -> Result<Vec<T>, Error> {
        let message = self.call_method(method, |m| m.append1(device_id))?;
        let iter: Array<Dict<String, Variant<Box<RefArg + 'static>>, _>, _> = message
            .read1()
            .map_err(|why| Error::ArgumentMismatch(method, why))?;

        Ok(iter.map(T::from_iter).collect())
    }

    fn get_handle_method<H: IntoRawFd>(
        &self,
        method: &'static str,
        handle: H,
    ) -> Result<Vec<HashMap<String, DynVariant>>, Error> {
        let cb = move |m: Message| m.append1(OwnedFd::new(handle.into_raw_fd()));

        self.call_method(method, cb)?
            .read1()
            .map_err(|why| Error::ArgumentMismatch(method, why))
    }

    fn call_method<F: FnOnce(Message) -> Message>(
        &self,
        method: &'static str,
        append_args: F,
    ) -> Result<Message, Error> {
        let mut m = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, method)
            .map_err(|why| Error::NewMethodCall(method, why))?;

        m = append_args(m);

        self.send_with_reply_and_block(m, TIMEOUT)
            .map_err(|why| Error::Call(method, why))
    }
}

/// Signal received by the daemon when listening for signal events with `Client::listen_signals()`.
pub enum Signal {
    /// Some value on the interface or the number of devices or profiles has changed.
    Changed,
    /// A device has been added.
    DeviceAdded(Device),
    /// A device has been changed.
    DeviceChanged(Device),
    /// A device has been removed.
    DeviceRemoved(Device),
}
