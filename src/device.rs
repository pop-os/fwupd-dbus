use crate::dbus_helpers::*;
use crate::{Client, DBusEntry, DynVariant, Error};
use dbus::arg::RefArg;
use std::{collections::HashMap, iter::FromIterator};

// From libfwupd/fwupd-enums.h
bitflags! {
    pub struct DeviceFlags: u64 {
        /// Device cannot be removed easily
        const INTERNAL               = 1 << 0;
        /// Device is updatable in this or any other mode
        const UPDATABLE              = 1 << 1;
        /// Update can only be done from offline mode
        const ONLY_OFFLINE           = 1 << 2;
        /// Requires AC power
        const REQUIRE_AC             = 1 << 3;
        /// Is locked and can be unlocked
        const LOCKED                 = 1 << 4;
        /// Is found in current metadata
        const SUPPORTED              = 1 << 5;
        /// Requires a bootloader mode to be manually enabled by the user
        const NEEDS_BOOTLOADER       = 1 << 6;
        /// Has been registered with other plugins
        const REGISTERED             = 1 << 7;
        /// Requires a reboot to apply firmware or to reload hardware
        const NEEDS_REBOOT           = 1 << 8;
        /// Has been reported to a metadata server
        const REPORTED               = 1 << 9;
        /// User has been notified
        const NOTIFIED               = 1 << 10;
        /// Always use the runtime version rather than the bootloader
        const USE_RUNTIME_VERSION    = 1 << 11;
        /// Install composite firmware on the parent before the child
        const INSTALL_PARENT_FIRST   = 1 << 12;
        /// Is currently in bootloader mode
        const IS_BOOTLOADER          = 1 << 13;
        /// The hardware is waiting to be replugged
        const WAIT_FOR_REPLUG        = 1 << 14;
        /// Ignore validation safety checks when flashing this device
        const IGNORE_VALIDATION      = 1 << 15;
        /// Extra metadata can be exposed about this device
        const TRUSTED                = 1 << 16;
        /// Requires system shutdown to apply firmware
        const NEEDS_SHUTDOWN         = 1 << 17;
        /// Requires the update to be retried with a new plugin
        const ANOTHER_WRITE_REQUIRED = 1 << 18;
        /// Do not add instance IDs from the device baseclass
        const NO_AUTO_INSTANCE_IDS   = 1 << 19;
        /// Device update needs to be separately activated
        const NEEDS_ACTIVATION       = 1 << 20;
        /// Ensure the version is a valid semantic version, e.g. numbers separated with dots
        const ENSURE_SEMVER          = 1 << 21;
        const UNKNOWN                = std::u64::MAX;
    }
}

impl Default for DeviceFlags {
    fn default() -> Self {
        DeviceFlags::empty()
    }
}

/// A device that is potentially-supported by fwupd.
#[derive(Debug, Default)]
pub struct Device {
    pub created: u64,
    pub device_id: Box<str>,
    pub flags: DeviceFlags,
    pub guid: Vec<Box<str>>,
    pub icon: Vec<Box<str>>,
    pub instance_ids: Vec<Box<str>>,
    pub name: Box<str>,
    pub plugin: Box<str>,
    pub update_error: Option<Box<str>>,
    pub vendor_id: Box<str>,
    pub vendor: Box<str>,
    pub version: Box<str>,
}

impl Device {
    /// Get a list of all the downgrades possible for thisdevice.
    pub fn downgrades(&self, client: &Client) -> Result<Vec<HashMap<String, DynVariant>>, Error> {
        client.get_downgrades(&self.device_id)
    }

    /// Get a list of all the upgrades possible for this device.
    pub fn upgrades(&self, client: &Client) -> Result<Vec<HashMap<String, DynVariant>>, Error> {
        client.get_upgrades(&self.device_id)
    }

    /// Gets a list of all the releases for this device.
    pub fn releases(&self, client: &Client) -> Result<Vec<HashMap<String, DynVariant>>, Error> {
        client.get_releases(&self.device_id)
    }

    /// Determins if the device is updateable or not.
    pub fn is_updateable(&self) -> bool {
        self.flags.contains(DeviceFlags::UPDATABLE)
    }

    /// Checks if the device requires a reboot.
    pub fn needs_reboot(&self) -> bool {
        self.flags.contains(DeviceFlags::NEEDS_REBOOT)
    }
}

impl FromIterator<DBusEntry> for Device {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = DBusEntry>,
    {
        let mut device = Device::default();

        for (key, value) in iter {
            let key = key.as_str();
            match key {
                "Created" => device.created = dbus_u64(&value, key).into(),
                "DeviceId" => device.device_id = dbus_str(&value, key).into(),
                "Flags" => device.flags = DeviceFlags::from_bits_truncate(dbus_u64(&value, key)),
                "Guid" => {
                    device.guid = value
                        .as_iter()
                        .expect("Guid is not a variant")
                        .flat_map(|array| array.as_iter().expect("Guid is not an iterator"))
                        .map(|elem| dbus_str(elem, key).into())
                        .collect::<Vec<Box<str>>>()
                }
                "Icon" => {
                    device.icon = value
                        .as_iter()
                        .expect("Icon is not a variant")
                        .flat_map(|array| array.as_iter().expect("Icon is not an iterator"))
                        .map(|elem| dbus_str(elem, key).into())
                        .collect::<Vec<Box<str>>>()
                }
                "InstanceIds" => {
                    device.instance_ids = value
                        .as_iter()
                        .expect("InstanceIds is not a variant")
                        .flat_map(|array| array.as_iter().expect("InstanceIds is not an iterator"))
                        .map(|value| dbus_str(value, key).into())
                        .collect::<Vec<Box<str>>>()
                }
                "Name" => device.name = dbus_str(&value, key).into(),
                "Plugin" => device.plugin = dbus_str(&value, key).into(),
                "UpdateError" => device.update_error = Some(dbus_str(&value, key).into()),
                "Vendor" => device.vendor = dbus_str(&value, key).into(),
                "VendorId" => device.vendor_id = dbus_str(&value, key).into(),
                "Version" => device.version = dbus_str(&value, key).into(),
                other => {
                    eprintln!("unknown remote key: {} ({})", other, value.signature());
                }
            }
        }

        device
    }
}
