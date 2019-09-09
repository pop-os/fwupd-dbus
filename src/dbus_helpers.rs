use dbus::arg::RefArg;

pub fn dbus_str<'a>(variant: &'a dyn RefArg, kind: &str) -> &'a str {
    variant
        .as_str()
        .unwrap_or_else(|| panic!("expected str for {}, found {}", kind, variant.signature()))
}

pub fn dbus_u64(variant: &dyn RefArg, kind: &str) -> u64 {
    variant
        .as_u64()
        .unwrap_or_else(|| panic!("expected u64 for {}, found {}", kind, variant.signature()))
}

pub fn dbus_i64(variant: &dyn RefArg, kind: &str) -> i64 {
    variant
        .as_i64()
        .unwrap_or_else(|| panic!("expected i64 for {}, found {}", kind, variant.signature()))
}
