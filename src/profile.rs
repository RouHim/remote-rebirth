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
    /// How many infrared groups the skin's configurations allocate, empty ones included.
    pub ir_groups: usize,
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
    /// The scan code of each Device Key, in place order: top left, top right, bottom left, bottom
    /// right, then any further place a wider model family states.
    pub device_key_scans: &'static [u8],
    /// What each Device Key's place is printed as, in the same order as `device_key_scans`.
    pub device_key_labels: &'static [&'static str],
}

pub const HARMONY_300_SKINS: &[SkinProfile] = &[
    SkinProfile {
        skin: 78,
        favourite_capacity: 5,
        // `reference/remotes/harmony-300/keys.md`: favourite keys 1, 2, 3, 4 and 5 raise scans 8, 9, 11,
        // 34 and 2 on the US printing, whose middle key is Teletext's place on the EMEA one.
        favourite_scans: &[8, 9, 11, 34, 2],
        ir_groups: 4,
        label: "Harmony 300",
    },
    SkinProfile {
        skin: 79,
        favourite_capacity: 4,
        // The EMEA printing's favourite keys 1, 2, 3 and 4 raise scans 8, 9, 2 and 34.
        favourite_scans: &[8, 9, 2, 34],
        ir_groups: 4,
        label: "Harmony 300 EMEA",
    },
];

/// The scan code of each Harmony 300 Device Key, in place order: top left, top right, bottom left,
/// bottom right.
pub const HARMONY_300_DEVICE_KEY_SCANS: &[u8] = &[5, 1, 7, 4];

/// What each Harmony 300 Device Key's place is printed as.
pub const HARMONY_300_DEVICE_KEY_LABELS: &[&str] = &["TV", "Cable/Sat", "DVD", "VCR/Aux"];

/// A Harmony 300/350 family name from the USB device descriptor's `bcdDevice`, which the shared
/// product ID cannot state.
pub fn family(bcd_device: u16) -> Option<&'static str> {
    match bcd_device {
        0x1078 => Some("Harmony 300"),
        0x1104 => Some("Harmony 350"),
        _ => None,
    }
}

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
    device_key_scans: HARMONY_300_DEVICE_KEY_SCANS,
    device_key_labels: HARMONY_300_DEVICE_KEY_LABELS,
};

/// A test-only profile shaped like the Harmony 350: architecture 16, a fifth Device Key and five
/// infrared groups. It proves Detect, Compose and validation read their capabilities from the
/// profile rather than from the codec. It is registered only in test builds.
///
/// Its Skin is 105 rather than the real 350's 104 so that the production posture — 104 is
/// recognised but has no Model Profile — stays observable in the same test build.
#[cfg(test)]
pub const HARMONY_350_TEST_SKINS: &[SkinProfile] = &[SkinProfile {
    skin: 105,
    favourite_capacity: 5,
    favourite_scans: &[8, 9, 11, 34, 2],
    ir_groups: 5,
    label: "Harmony 350",
}];

#[cfg(test)]
pub const HARMONY_350_TEST: ModelProfile = ModelProfile {
    id: "harmony_350_test",
    model: "Harmony 350",
    product_id: 0xC124,
    architecture: 16,
    firmware: "1.4",
    skins: HARMONY_350_TEST_SKINS,
    max_devices: 5,
    max_ir_groups: 5,
    long_press: false,
    key_layout: HARMONY_300_KEYS,
    device_key_scans: &[5, 1, 7, 4, 25],
    device_key_labels: &["TV", "Cable/Sat", "DVD", "VCR/Aux", "Aux 2"],
};

/// The profiles this build knows, in preference order.
const PROFILES: &[&ModelProfile] = &[&HARMONY_300];
#[cfg(test)]
const TEST_PROFILES: &[&ModelProfile] = &[&HARMONY_350_TEST];
#[cfg(not(test))]
const TEST_PROFILES: &[&ModelProfile] = &[];

/// Every registered Model Profile.
pub fn all() -> impl Iterator<Item = &'static ModelProfile> {
    PROFILES.iter().chain(TEST_PROFILES.iter()).copied()
}

pub fn by_id(id: &str) -> Option<&'static ModelProfile> {
    all().find(|profile| profile.id == id)
}

pub fn for_identity(architecture: Option<u8>, skin: Option<u8>) -> Option<&'static ModelProfile> {
    let architecture = architecture?;
    let skin = skin?;
    all().find(|profile| {
        profile.architecture == architecture && profile.skins.iter().any(|entry| entry.skin == skin)
    })
}

/// The Model Profile whose architecture states `skin`, or the first profile whose skins include it.
pub fn for_skin(skin: u8) -> Option<&'static ModelProfile> {
    all().find(|profile| profile.skins.iter().any(|entry| entry.skin == skin))
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

/// The positional Favourite key whose scan code is `scan`, or `None` where the skin prints no
/// Favourite there.
pub fn favourite_key(profile: &ModelProfile, skin: u8, scan: u8) -> Option<u8> {
    let skin = skin_profile(profile, skin)?;
    skin.favourite_scans
        .iter()
        .position(|entry| *entry == scan)
        .map(|place| place as u8 + 1)
}

/// How many infrared groups a skin's configurations allocate, empty ones included.
///
/// Skins 78 and 79 are the Harmony 300's two printings, with four groups; skin 104 is the Harmony
/// 350, with eight. Any other skin is refused by the caller, which names the skins that are known.
pub fn empty_ir_groups(skin: u8) -> Option<usize> {
    all()
        .flat_map(|profile| profile.skins)
        .find(|entry| entry.skin == skin)
        .map(|entry| entry.ir_groups)
        .or_else(|| {
            UNPROFILED_SKIN_IR_GROUPS
                .iter()
                .find(|(measured, _)| *measured == skin)
                .map(|(_, groups)| *groups)
        })
}

/// The measured allocations of skins whose model has no profile yet. Only the empty configuration
/// needs them, because it states a group count but no capability.
const UNPROFILED_SKIN_IR_GROUPS: &[(u8, usize)] = &[(104, 8)];
