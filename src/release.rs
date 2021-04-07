use crate::{common::*, dbus_helpers::*, DBusEntry, RemoteId};
use dbus::arg::RefArg;
use std::{cmp::Ordering, iter::FromIterator};

bitflags! {
    /// Describes attributes of a release.
    pub struct ReleaseFlags: u64 {
        const TRUSTED_PAYLOAD  = 1;
        const TRUSTED_METADATA = 1 << 1;
        const IS_UPGRADE       = 1 << 2;
        const IS_DOWNGRADE     = 1 << 3;
        const BLOCKED_VERSION  = 1 << 4;
        const BLOCKED_APPROVAL = 1 << 5;
        const UNKNOWN          = std::u64::MAX;
    }
}

impl Default for ReleaseFlags {
    fn default() -> Self { ReleaseFlags::empty() }
}

bitflags! {
    /// Describes trust levels for the payload and/or metadata.
    pub struct TrustFlags: u64 {
        const PAYLOAD  = 1;
        const METADATA = 1 << 1;
    }
}

impl Default for TrustFlags {
    fn default() -> Self { TrustFlags::empty() }
}

/// Information about an available fwupd remote.
#[derive(Clone, Debug, Default, Eq)]
pub struct Release {
    pub appstream_id:     Box<str>,
    pub categories:       Box<[Box<str>]>,
    pub checksums:        Box<[Box<str>]>,
    pub created:          u64,
    pub description:      Box<str>,
    pub details_url:      Option<Box<str>>,
    pub filename:         Box<str>,
    pub flags:            ReleaseFlags,
    pub homepage:         Box<str>,
    pub install_duration: u32,
    pub license:          Box<str>,
    pub name:             Box<str>,
    pub protocol:         Option<Box<str>>,
    pub remote_id:        RemoteId,
    pub size:             u64,
    pub source_url:       Option<Box<str>>,
    pub summary:          Box<str>,
    pub trust_flags:      TrustFlags,
    pub update_message:   Option<Box<str>>,
    pub uri:              Box<str>,
    pub vendor:           Box<str>,
    pub version:          Box<str>,
}

impl Ord for Release {
    fn cmp(&self, other: &Self) -> Ordering { self.version.cmp(&other.version) }
}

impl PartialOrd for Release {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl PartialEq for Release {
    fn eq(&self, other: &Self) -> bool { self.version == other.version }
}

impl AsRef<RemoteId> for Release {
    fn as_ref(&self) -> &RemoteId { &self.remote_id }
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
                KEY_APPSTREAM_ID => release.appstream_id = dbus_str(&value, key).into(),
                KEY_CATEGORIES => {
                    release.categories = value
                        .as_iter()
                        .expect("Categories is not a variant")
                        .flat_map(|array| array.as_iter().expect("Categories is not an iterator"))
                        .map(|value| dbus_str(&value, key).into())
                        .collect::<Vec<Box<str>>>()
                        .into_boxed_slice()
                }
                KEY_CHECKSUM => {
                    release.checksums = value
                        .as_iter()
                        .expect("Checksums is not a variant")
                        .map(|value| dbus_str(&value, key).into())
                        .flat_map(|value: Box<str>| {
                            if value.contains(',') {
                                value.split(',').map(|x| x.into()).collect()
                            } else {
                                vec![value]
                            }
                        })
                        .collect::<Vec<Box<str>>>()
                        .into_boxed_slice()
                }
                KEY_DESCRIPTION => release.description = dbus_str(&value, key).into(),
                KEY_DETAILS_URL => release.details_url = Some(dbus_str(&value, key).into()),
                KEY_FILENAME => release.filename = dbus_str(&value, key).into(),
                KEY_FLAGS => {
                    release.flags = ReleaseFlags::from_bits_truncate(dbus_u64(&value, key))
                }
                KEY_HOMEPAGE => release.homepage = dbus_str(&value, key).into(),
                KEY_INSTALL_DURATION => release.install_duration = dbus_u64(&value, key) as u32,
                KEY_LICENSE => release.license = dbus_str(&value, key).into(),
                // KEY_METADATA => (),
                KEY_NAME => release.name = dbus_str(&value, key).into(),
                KEY_PROTOCOL => release.protocol = Some(dbus_str(&value, key).into()),
                KEY_REMOTE_ID => release.remote_id = RemoteId(dbus_str(&value, key).into()),
                KEY_SIZE => release.size = dbus_u64(&value, key),
                KEY_SOURCE_URL => release.source_url = Some(dbus_str(&value, key).into()),
                KEY_SUMMARY => release.summary = dbus_str(&value, key).into(),
                KEY_TRUST_FLAGS => {
                    release.trust_flags = TrustFlags::from_bits_truncate(dbus_u64(&value, key))
                }
                KEY_UPDATE_MESSAGE => release.update_message = Some(dbus_str(&value, key).into()),
                KEY_URI => release.uri = dbus_str(&value, key).into(),
                KEY_VENDOR => release.vendor = dbus_str(&value, key).into(),
                KEY_VERSION => release.version = dbus_str(&value, key).into(),
                other => {
                    eprintln!("unknown release key: {} ({})", other, value.signature());
                }
            }
        }

        release
    }
}
