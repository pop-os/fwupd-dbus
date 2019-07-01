use crate::dbus_helpers::*;
use crate::DBusEntry;
use dbus::arg::RefArg;
use std::iter::FromIterator;

/// Information about an available fwupd remote.
#[derive(Debug, Default)]
pub struct Remote {
    pub _type: u16,
    pub agreement: Box<str>,
    pub checksum: Option<Box<str>>,
    pub enabled: bool,
    pub filename_cache: Box<str>,
    pub filename_source: Box<str>,
    pub keyring: u16,
    pub modification_time: u64,
    pub priority: i16,
    pub remote_id: Box<str>,
    pub report_uri: Box<str>,
    pub title: Box<str>,
    pub uri: Box<str>,
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
                "Agreement" => remote.agreement = dbus_str(&value, key).into(),
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
                "ReportUri" => remote.report_uri = dbus_str(&value, key).into(),
                "Title" => remote.title = dbus_str(&value, key).into(),
                "Type" => remote._type = dbus_u64(&value, key) as u16,
                "Uri" => remote.uri = dbus_str(&value, key).into(),
                other => {
                    eprintln!("unknown remote key: {} ({})", other, value.signature());
                }
            }
        }

        remote
    }
}
