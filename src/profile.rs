use serde::Serialize;

/// One canonical key name and the scan code the profile's keypad raises for it.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct KeyBinding {
    pub name: &'static str,
    pub scan: u8,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SkinProfile {
    pub skin: u8,
    pub favourite_capacity: usize,
    /// The scan code of each positional favourite key, in key order.
    pub favourite_scans: &'static [u8],
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
    /// The keys a model's canonical key names may bind, and the scan code each raises.
    pub key_layout: &'static [KeyBinding],
}

pub const HARMONY_300_SKINS: &[SkinProfile] = &[
    SkinProfile {
        skin: 78,
        favourite_capacity: 5,
        // `reference/remotes/harmony-300/keys.md`: favourite keys 1, 2, 3, 4 and 5 raise scans 8, 9, 11,
        // 34 and 2 on the US printing, whose middle key is Teletext's place on the EMEA one.
        favourite_scans: &[8, 9, 11, 34, 2],
        label: "Harmony 300",
    },
    SkinProfile {
        skin: 79,
        favourite_capacity: 4,
        // The EMEA printing's favourite keys 1, 2, 3 and 4 raise scans 8, 9, 2 and 34.
        favourite_scans: &[8, 9, 2, 34],
        label: "Harmony 300 EMEA",
    },
];

/// The Harmony 300's keypad as a composed device meets it, from
/// `reference/remotes/harmony-300/keys.md` and the Harmony 350's table it shares a moulding with:
/// every canonical key except `live`, whose two candidate scans no configuration decides.
pub const HARMONY_300_KEYS: &[KeyBinding] = &[
    KeyBinding {
        name: "power",
        scan: 3,
    },
    KeyBinding {
        name: "volume_up",
        scan: 49,
    },
    KeyBinding {
        name: "volume_down",
        scan: 52,
    },
    KeyBinding {
        name: "volume_mute",
        scan: 15,
    },
    KeyBinding {
        name: "channel_up",
        scan: 55,
    },
    KeyBinding {
        name: "channel_down",
        scan: 41,
    },
    KeyBinding {
        name: "number_0",
        scan: 20,
    },
    KeyBinding {
        name: "number_1",
        scan: 24,
    },
    KeyBinding {
        name: "number_2",
        scan: 47,
    },
    KeyBinding {
        name: "number_3",
        scan: 39,
    },
    KeyBinding {
        name: "number_4",
        scan: 17,
    },
    KeyBinding {
        name: "number_5",
        scan: 48,
    },
    KeyBinding {
        name: "number_6",
        scan: 37,
    },
    KeyBinding {
        name: "number_7",
        scan: 18,
    },
    KeyBinding {
        name: "number_8",
        scan: 53,
    },
    KeyBinding {
        name: "number_9",
        scan: 45,
    },
    KeyBinding {
        name: "direction_up",
        scan: 51,
    },
    KeyBinding {
        name: "direction_down",
        scan: 42,
    },
    KeyBinding {
        name: "direction_left",
        scan: 14,
    },
    KeyBinding {
        name: "direction_right",
        scan: 31,
    },
    KeyBinding {
        name: "select",
        scan: 50,
    },
    KeyBinding {
        name: "menu",
        scan: 10,
    },
    KeyBinding {
        name: "exit",
        scan: 12,
    },
    KeyBinding {
        name: "info",
        scan: 36,
    },
    KeyBinding {
        name: "guide",
        scan: 33,
    },
    KeyBinding {
        name: "play",
        scan: 44,
    },
    KeyBinding {
        name: "pause",
        scan: 46,
    },
    KeyBinding {
        name: "stop",
        scan: 40,
    },
    KeyBinding {
        name: "rewind",
        scan: 22,
    },
    KeyBinding {
        name: "fast_forward",
        scan: 30,
    },
    KeyBinding {
        name: "skip_back",
        scan: 21,
    },
    KeyBinding {
        name: "skip_forward",
        scan: 38,
    },
    KeyBinding {
        name: "record",
        scan: 23,
    },
    KeyBinding {
        name: "red",
        scan: 13,
    },
    KeyBinding {
        name: "green",
        scan: 27,
    },
    KeyBinding {
        name: "yellow",
        scan: 28,
    },
    KeyBinding {
        name: "blue",
        scan: 29,
    },
    KeyBinding {
        name: "list",
        scan: 16,
    },
    KeyBinding {
        name: "prev_channel",
        scan: 32,
    },
    KeyBinding {
        name: "clear",
        scan: 19,
    },
    KeyBinding {
        name: "enter",
        scan: 54,
    },
    KeyBinding {
        name: "input",
        scan: 35,
    },
    KeyBinding {
        name: "teletext",
        scan: 11,
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
    key_layout: HARMONY_300_KEYS,
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

/// The scan code the profile's keypad raises for `name`, or `None` where the profile states none.
pub fn key_scan(profile: &ModelProfile, name: &str) -> Option<u8> {
    profile
        .key_layout
        .iter()
        .find(|binding| binding.name == name)
        .map(|binding| binding.scan)
}
