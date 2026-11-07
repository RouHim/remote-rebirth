#[derive(Debug, Clone, Default)]
pub struct SysInfo {
    pub architecture: Option<u8>,
    pub skin: Option<u8>,
    pub firmware_version: Option<String>,
    pub hardware_version: Option<String>,
    pub guid: Option<String>,
    pub serial: Option<String>,
    pub link_packet_length: Option<u16>,
}

pub fn parse(bytes: &[u8]) -> SysInfo {
    let text = String::from_utf8_lossy(bytes);
    let mut info = SysInfo::default();
    for line in text.lines() {
        let line = line.trim_matches('\0').trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = split_line(line);
        let key = normalize_key(key);
        let value = value.trim().to_string();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        match key.as_str() {
            "arch" | "architecture" => info.architecture = parse_u8(&value),
            "skin" => info.skin = parse_u8(&value),
            "fw_ver" | "firmware_version" => {
                info.firmware_version = Some(normalize_version(&value));
            }
            "hw_ver" | "hardware_version" => {
                info.hardware_version = Some(normalize_version(&value));
            }
            "guid" => info.guid = Some(value),
            "serial" | "serial_number" | "serialnumber" => info.serial = Some(value),
            "link_packet_length" => info.link_packet_length = parse_u16(&value),
            _ => {}
        }
    }
    info
}

fn split_line(line: &str) -> (&str, &str) {
    if let Some((key, value)) = line.split_once('=') {
        return (key, value);
    }
    if let Some((key, value)) = line.split_once(':') {
        return (key, value);
    }
    line.split_once(char::is_whitespace).unwrap_or((line, ""))
}

fn normalize_key(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

fn parse_u8(value: &str) -> Option<u8> {
    parse_u64(value).and_then(|value| u8::try_from(value).ok())
}

fn parse_u16(value: &str) -> Option<u16> {
    parse_u64(value).and_then(|value| u16::try_from(value).ok())
}

fn parse_u64(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn normalize_version(value: &str) -> String {
    if (value.starts_with("0x") || value.starts_with("0X"))
        && let Some(byte) = parse_u8(value)
    {
        return format!("{}.{}", byte >> 4, byte & 0x0F);
    }
    value.to_string()
}
