use crate::common::*;
use crate::dbus_helpers::*;
use crate::DBusEntry;
use dbus::arg::RefArg;
use std::iter::FromIterator;

bitflags! {
    /// Describes attributes of a device.
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

/// Describes the state of the last update on a device.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum UpdateState {
    Unknown,
    Pending,
    Success,
    Failed,
    NeedsReboot,
    FailedTransient,
}

impl From<u8> for UpdateState {
    fn from(value: u8) -> Self {
        use self::UpdateState::*;
        match value {
            0 => Unknown,
            1 => Pending,
            2 => Success,
            3 => Failed,
            4 => NeedsReboot,
            5 => FailedTransient,
            _ => Unknown,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum VersionFormat {
    Unknown,
    Plain,
    Number,
    Pair,
    Triplet,
    Quad,
    Bcd,
    IntelMe,
    IntelMe2,
}

impl From<u8> for VersionFormat {
    fn from(value: u8) -> Self {
        use self::VersionFormat::*;
        match value {
            0 => Unknown,
            1 => Plain,
            2 => Number,
            3 => Pair,
            4 => Triplet,
            5 => Quad,
            6 => Bcd,
            7 => IntelMe,
            8 => IntelMe2,
            _ => Unknown,
        }
    }
}

/// The remote ID of a device.
#[derive(Clone, Debug, Default, Shrinkwrap)]
pub struct DeviceId(Box<str>);

/// A device that is potentially-supported by fwupd.
#[derive(Debug, Default)]
pub struct Device {
    pub checksum: Option<Box<str>>,
    pub created: u64,
    pub description: Option<Box<str>>,
    pub device_id: DeviceId,
    pub flags: DeviceFlags,
    pub flashes_left: Option<u32>,
    pub guid: Box<[Box<str>]>,
    pub icon: Box<[Box<str>]>,
    pub install_duration: Option<u32>,
    pub instance_ids: Box<[Box<str>]>,
    pub modified: Option<u64>,
    pub name: Box<str>,
    pub parent_device_id: Option<DeviceId>,
    pub plugin: Box<str>,
    pub serial: Option<Box<str>>,
    pub summary: Option<Box<str>>,
    pub update_error: Option<Box<str>>,
    pub update_message: Option<Box<str>>,
    pub update_state: Option<UpdateState>,
    pub vendor_id: Box<str>,
    pub vendor: Box<str>,
    pub version_bootloader: Option<Box<str>>,
    pub version_format: Option<VersionFormat>,
    pub version_lowest: Option<Box<str>>,
    pub version: Box<str>,
}

impl Device {
    /// Check if the given `DeviceFlag` is set.
    pub fn has_flag(&self, flags: DeviceFlags) -> bool {
        self.flags.contains(flags)
    }

    /// Returns true if a GUID match was found.
    pub fn has_guid(&self, guid: &str) -> bool {
        self.guid.iter().any(|g| g.as_ref() == guid)
    }

    /// Checks if the device is supported by fwupd.
    pub fn is_supported(&self) -> bool {
        self.has_flag(DeviceFlags::SUPPORTED)
    }

    /// Determins if the device is updateable or not.
    pub fn is_updateable(&self) -> bool {
        self.has_flag(DeviceFlags::UPDATABLE)
    }

    /// Checks if the device requires a reboot.
    pub fn needs_reboot(&self) -> bool {
        self.has_flag(DeviceFlags::NEEDS_REBOOT)
    }

    /// Check if the device must be updated offline.
    pub fn only_offline(&self) -> bool {
        self.has_flag(DeviceFlags::ONLY_OFFLINE)
    }
}

impl AsRef<DeviceId> for Device {
    fn as_ref(&self) -> &DeviceId {
        &self.device_id
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
                KEY_CHECKSUM => device.checksum = Some(dbus_str(&value, key).into()),
                KEY_CREATED => device.created = dbus_u64(&value, key).into(),
                KEY_DESCRIPTION => device.description = Some(dbus_str(&value, key).into()),
                KEY_DEVICE_ID => device.device_id = DeviceId(dbus_str(&value, key).into()),
                KEY_FLAGS => device.flags = DeviceFlags::from_bits_truncate(dbus_u64(&value, key)),
                KEY_FLASHES_LEFT => device.flashes_left = Some(dbus_u64(&value, key) as u32),
                KEY_GUID => {
                    device.guid = value
                        .as_iter()
                        .expect("Guid is not a variant")
                        .flat_map(|array| array.as_iter().expect("Guid is not an iterator"))
                        .map(|elem| dbus_str(elem, key).into())
                        .collect::<Vec<Box<str>>>()
                        .into_boxed_slice()
                }
                KEY_ICON => {
                    device.icon = value
                        .as_iter()
                        .expect("Icon is not a variant")
                        .flat_map(|array| array.as_iter().expect("Icon is not an iterator"))
                        .map(|elem| dbus_str(elem, key).into())
                        .collect::<Vec<Box<str>>>()
                        .into_boxed_slice()
                }
                KEY_INSTALL_DURATION => {
                    device.install_duration = Some(dbus_u64(&value, key) as u32)
                }
                KEY_INSTANCE_IDS => {
                    device.instance_ids = value
                        .as_iter()
                        .expect("InstanceIds is not a variant")
                        .flat_map(|array| array.as_iter().expect("InstanceIds is not an iterator"))
                        .map(|value| dbus_str(value, key).into())
                        .collect::<Vec<Box<str>>>()
                        .into_boxed_slice()
                }
                KEY_MODIFIED => device.modified = Some(dbus_u64(&value, key)),
                KEY_NAME => device.name = dbus_str(&value, key).into(),
                KEY_PARENT_DEVICE_ID => {
                    device.parent_device_id = Some(DeviceId(dbus_str(&value, key).into()))
                }
                KEY_PLUGIN => device.plugin = dbus_str(&value, key).into(),
                KEY_SERIAL => device.serial = Some(dbus_str(&value, key).into()),
                KEY_SUMMARY => device.summary = Some(dbus_str(&value, key).into()),
                KEY_UPDATE_ERROR => device.update_error = Some(dbus_str(&value, key).into()),
                KEY_UPDATE_MESSAGE => device.update_message = Some(dbus_str(&value, key).into()),
                KEY_UPDATE_STATE => {
                    device.update_state = Some(UpdateState::from(dbus_u64(&value, key) as u8))
                }
                KEY_VENDOR => device.vendor = dbus_str(&value, key).into(),
                KEY_VENDOR_ID => device.vendor_id = dbus_str(&value, key).into(),
                KEY_VERSION => device.version = dbus_str(&value, key).into(),
                KEY_VERSION_BOOTLOADER => {
                    device.version_bootloader = Some(dbus_str(&value, key).into())
                }
                KEY_VERSION_LOWEST => device.version_lowest = Some(dbus_str(&value, key).into()),
                "VersionFormat" => {
                    device.version_format = Some(VersionFormat::from(dbus_u64(&value, key) as u8))
                }
                other => {
                    eprintln!(
                        "unknown device key: {} ({}): {:?}",
                        other,
                        value.signature(),
                        value,
                    );
                }
            }
        }

        device
    }
}
