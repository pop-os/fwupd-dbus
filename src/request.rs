#[derive(Clone, Debug, Default)]
pub struct Request {
    pub appstream_id:   String,
    pub created:        u64,
    pub plugin:         String,
    pub request_kind:   u32,
    pub update_message: String,
}
