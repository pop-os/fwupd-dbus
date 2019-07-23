use dbus::arg::RefArg;

pub fn dbus_str<'a>(variant: &'a dyn RefArg, kind: &str) -> &'a str {
    variant.as_str().expect(&format!("expected str for {}, found {}", kind, variant.signature()))
}

pub fn dbus_u64(variant: &dyn RefArg, kind: &str) -> u64 {
    variant.as_u64().expect(&format!("expected u64 for {}, found {}", kind, variant.signature()))
}

pub fn dbus_i64(variant: &dyn RefArg, kind: &str) -> i64 {
    variant.as_i64().expect(&format!("expected i64 for {}, found {}", kind, variant.signature()))
}
