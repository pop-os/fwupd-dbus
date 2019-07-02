use crate::dbus_helpers::*;
use crate::DBusEntry;
use dbus::arg::RefArg;
use std::iter::FromIterator;

bitflags! {
    pub struct ReleaseFlags: u64 {
        const TRUSTED_PAYLOAD  = 1 << 0;
        const TRUSTED_METADATA = 1 << 1;
        const IS_UPGRADE       = 1 << 2;
        const IS_DOWNGRADE     = 1 << 3;
        const BLOCKED_VERSION  = 1 << 4;
        const BLOCKED_APPROVAL = 1 << 5;
        const UNKNOWN          = std::u64::MAX;
    }
}

impl Default for ReleaseFlags {
    fn default() -> Self {
        ReleaseFlags::empty()
    }
}

/// Information about an available fwupd remote.
#[derive(Debug, Default)]
pub struct Release {
    appstream_id: Box<str>,
    categories: Box<[Box<str>]>,
    checksums: Box<[Box<str>]>,
    description: Box<str>,
    details_url: Box<str>,
    filename: Box<str>,
    flags: ReleaseFlags,
    homepage: Box<str>,
    install_duration: u32,
    license: Box<str>,
    name: Box<str>,
    protocol: Box<str>,
    remote_id: Box<str>,
    size: u64,
    source_url: Box<str>,
    summary: Box<str>,
    update_message: Box<str>,
    uri: Box<str>,
    vendor: Box<str>,
    version: Box<str>,
}

impl FromIterator<DBusEntry> for Release {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = DBusEntry>,
    {
        let mut release = Release::default();

        for (key, value) in iter {
            let key = key.as_str();
            match key {
                "AppstreamId" => release.appstream_id = dbus_str(&value, key).into(),
                "Categories" => {
                    release.categories = value
                        .as_iter()
                        .expect("Categories is not a variant")
                        .flat_map(|array| array.as_iter().expect("Categories is not an iterator"))
                        .map(|value| dbus_str(&value, key).into())
                        .collect::<Vec<Box<str>>>()
                        .into_boxed_slice()
                }
                "Checksums" => {
                    release.checksums = value
                        .as_iter()
                        .expect("Checksums is not a variant")
                        .flat_map(|array| array.as_iter().expect("Checksums is not an iterator"))
                        .map(|value| dbus_str(&value, key).into())
                        .collect::<Vec<Box<str>>>()
                        .into_boxed_slice()
                }
                "Description" => release.description = dbus_str(&value, key).into(),
                "DetailsUrl" => release.details_url = dbus_str(&value, key).into(),
                "filename" => release.filename = dbus_str(&value, key).into(),
                "flags" => release.flags = ReleaseFlags::from_bits_truncate(dbus_u64(&value, key)),
                "Homepage" => release.homepage = dbus_str(&value, key).into(),
                "InstallDuration" => release.install_duration = dbus_u64(&value, key) as u32,
                "License" => release.license = dbus_str(&value, key).into(),
                "Size" => release.size = dbus_u64(&value, key),
                "SourceUrl" => release.source_url = dbus_str(&value, key).into(),
                "Summary" => release.summary = dbus_str(&value, key).into(),
                "UpdateMessage" => release.update_message = dbus_str(&value, key).into(),
                "Uri" => release.uri = dbus_str(&value, key).into(),
                "Vendor" => release.vendor = dbus_str(&value, key).into(),
                "Version" => release.version = dbus_str(&value, key).into(),
                other => {
                    eprintln!("unknown release key: {} ({})", other, value.signature());
                }
            }
        }

        release
    }
}
