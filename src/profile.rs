use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SkinProfile {
    pub skin: u8,
    pub favourite_capacity: usize,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelProfile {
    pub id: &'static str,
    pub model: &'static str,
    pub product_id: u16,
    pub architecture: u8,
    pub firmware: &'static str,
    pub skins: &'static [SkinProfile],
    pub max_devices: usize,
    pub max_ir_groups: usize,
    pub long_press: bool,
}

pub const HARMONY_300_SKINS: &[SkinProfile] = &[
    SkinProfile {
        skin: 78,
        favourite_capacity: 5,
        label: "Harmony 300",
    },
    SkinProfile {
        skin: 79,
        favourite_capacity: 4,
        label: "Harmony 300 EMEA",
    },
];

pub const HARMONY_300: ModelProfile = ModelProfile {
    id: "harmony_300",
    model: "Harmony 300",
    product_id: 0xC124,
    architecture: 16,
    firmware: "1.4",
    skins: HARMONY_300_SKINS,
    max_devices: 4,
    max_ir_groups: 4,
    long_press: false,
};

pub fn for_identity(architecture: Option<u8>, skin: Option<u8>) -> Option<&'static ModelProfile> {
    match (architecture, skin) {
        (Some(16), Some(78 | 79)) => Some(&HARMONY_300),
        _ => None,
    }
}

pub fn recognized_model(architecture: Option<u8>, skin: Option<u8>) -> Option<&'static str> {
    match (architecture, skin) {
        (Some(16), Some(78 | 79)) => Some("Harmony 300"),
        (Some(16), Some(104)) => Some("Harmony 350"),
        _ => None,
    }
}

pub fn skin_profile(profile: &ModelProfile, skin: u8) -> Option<&SkinProfile> {
    profile.skins.iter().find(|entry| entry.skin == skin)
}
