use crypto_hash::{Algorithm, Hasher};
use hex_view::HexView;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Based on libfwupd/fwupd-common.c
pub fn checksum_guess_kind(checksum: &str) -> Algorithm {
    match checksum.len() {
        32 => Algorithm::MD5,
        40 => Algorithm::SHA1,
        64 => Algorithm::SHA256,
        128 => Algorithm::SHA512,
        _ => Algorithm::SHA1,
    }
}

const ALGORITHMS: &[Algorithm] = &[Algorithm::SHA512, Algorithm::SHA256, Algorithm::SHA1];

/// Find the best checksum available for an array of checksums.
pub fn find_best_checksum<S: AsRef<str>>(checksums: &[S]) -> Option<(&str, Algorithm)> {
    for &algorithm in ALGORITHMS {
        for checksum in checksums {
            if algorithm == checksum_guess_kind(checksum.as_ref()) {
                return Some((checksum.as_ref(), algorithm));
            }
        }
    }

    None
}

pub fn validate_checksum<R: Read>(
    data: &mut R,
    checksum: &str,
    alg: Algorithm,
) -> io::Result<bool> {
    let mut hasher = Hasher::new(alg);
    io::copy(data, &mut hasher)?;
    let digest = format!("{:x}", HexView::from(hasher.finish().as_slice()));
    Ok(checksum == digest.as_str())
}

pub fn place_in_cache(file: &Path) -> PathBuf {
    xdg::BaseDirectories::with_prefix("fwupd-client")
        .expect("failed to get XDG base directories")
        .place_cache_file(file)
        .expect(&format!("failed to place {:?} in cache", file))
}

pub const KEY_APPSTREAM_ID: &str = "AppstreamId"; // s
pub const KEY_CATEGORIES: &str = "Categories"; // as
pub const KEY_CHECKSUM: &str = "Checksum"; // as
pub const KEY_CREATED: &str = "Created"; // t
pub const KEY_DESCRIPTION: &str = "Description"; // s
pub const KEY_DETAILS_URL: &str = "DetailsUrl"; // s
pub const KEY_DEVICE_ID: &str = "DeviceId"; // s
pub const KEY_FILENAME: &str = "Filename"; // s
pub const KEY_FLAGS: &str = "Flags"; // t
pub const KEY_FLASHES_LEFT: &str = "FlashesLeft"; // u
pub const KEY_GUID: &str = "Guid"; // as
pub const KEY_HOMEPAGE: &str = "Homepage"; // s
pub const KEY_ICON: &str = "Icon"; // as
pub const KEY_INSTALL_DURATION: &str = "InstallDuration"; // u
pub const KEY_INSTANCE_IDS: &str = "InstanceIds"; // as
pub const KEY_LICENSE: &str = "License"; // s
pub const KEY_METADATA: &str = "Metadata"; // a{ss}
pub const KEY_MODIFIED: &str = "Modified"; // t
pub const KEY_NAME: &str = "Name"; // s
pub const KEY_PARENT_DEVICE_ID: &str = "ParentDeviceId"; // s
pub const KEY_PLUGIN: &str = "Plugin"; // s
pub const KEY_PROTOCOL: &str = "Protocol"; // s
pub const KEY_RELEASE: &str = "Release"; // a{sv}
pub const KEY_REMOTE_ID: &str = "RemoteId"; // s
pub const KEY_SERIAL: &str = "Serial"; // s
pub const KEY_SIZE: &str = "Size"; // t
pub const KEY_SOURCE_URL: &str = "SourceUrl"; // s
pub const KEY_SUMMARY: &str = "Summary"; // s
pub const KEY_TRUST_FLAGS: &str = "TrustFlags"; // t
pub const KEY_UPDATE_ERROR: &str = "UpdateError"; // s
pub const KEY_UPDATE_MESSAGE: &str = "UpdateMessage"; // s
pub const KEY_UPDATE_STATE: &str = "UpdateState"; // u
pub const KEY_URI: &str = "Uri"; // s
pub const KEY_VENDOR_ID: &str = "VendorId"; // s
pub const KEY_VENDOR: &str = "Vendor"; // s
pub const KEY_VERSION_BOOTLOADER: &str = "VersionBootloader"; // s
pub const KEY_VERSION_LOWEST: &str = "VersionLowest"; // s
pub const KEY_VERSION: &str = "Version"; // s
