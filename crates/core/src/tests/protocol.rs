use super::*;

use crate::error::Error;
use crate::protocol::{DATA_PACKET_PAYLOAD, DONE_PACKET};

// ---------------------------------------------------------------------------
// Malformed-packet recovery.
// ---------------------------------------------------------------------------

#[cfg(feature = "cli")]
#[test]
fn write_recovers_a_fresh_session_after_a_malformed_packet() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().faults.fail_first_chunk = true;

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(5), "{stderr}");
    let calls = backend.calls().borrow().clone();
    let tail = &calls[mark..];
    assert_eq!(
        tail.iter()
            .filter(|call| matches!(call, Call::Open { .. }))
            .count(),
        2,
        "a malformed packet reopens a fresh session"
    );
    assert_eq!(
        tail.iter()
            .filter(|call| matches!(call, Call::OpenRead { path } if path == "/sys/sysinfo"))
            .count(),
        2,
        "the reopened session is used: the initial Detect read and the recovery read"
    );
    assert!(!tail.iter().any(|call| matches!(call, Call::Flush { .. })));

    let records = journal_records(directory.path());
    let steps: Vec<&str> = records
        .iter()
        .filter_map(|record| record["step"].as_str())
        .collect();
    assert_eq!(
        steps,
        vec!["prepare", "reset", "open_write", "failure", "recover"]
    );
    assert_eq!(
        records
            .iter()
            .find(|record| record["step"] == "recover")
            .and_then(|record| record["reply"].as_str()),
        Some("ok")
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_reports_a_combined_error_when_recovery_fails() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut capture = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut capture, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");

    // A fresh backend holds no earlier Open, so the first Open succeeds and only the recovery's
    // reopen is refused.
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    {
        let store = backend.store(0);
        let mut store = store.borrow_mut();
        store.faults.fail_first_chunk = true;
        store.faults.refuse_reopen = true;
    }

    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(5), "{stderr}");
    assert!(stderr.contains("recovery failed"), "{stderr}");
    assert!(stderr.contains("cannot be reopened"), "{stderr}");
    let records = journal_records(directory.path());
    assert_eq!(
        records
            .iter()
            .find(|record| record["step"] == "recover")
            .and_then(|record| record["reply"].as_str()),
        Some("failed")
    );
}

#[cfg(feature = "cli")]
#[test]
fn detect_prefers_a_harmony_over_another_logitech_device() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut mouse = FakeDevice {
        descriptor: descriptor(1, 6, 0xC077, 0x0100, "USB Optical Mouse"),
        sysinfo: Vec::new(),
        usercfg: Vec::new(),
    };
    mouse.descriptor.vendor_id = 0x046D;
    let (exit, stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124), mouse],
        &["detect", "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["supported"].as_bool(), Some(true));
    assert_eq!(value["model"].as_str(), Some("Harmony 300"));
}

#[test]
fn a_write_chunk_reply_is_read_at_the_sequence_offset() {
    let mut reply = [0u8; REPORT_BYTES];
    reply[0] = 0xFF;
    reply[1] = 0x03;
    reply[2] = 0xFE;
    assert!(chunk_reply_matches(&reply, 0, true));

    reply[2] = 0x80 | 5;
    assert!(chunk_reply_matches(&reply, 5, false));

    reply[0] = 0xFE;
    reply[2] = 0xFE;
    assert!(!chunk_reply_matches(&reply, 0, true));
}

struct ScriptedLink {
    replies: VecDeque<[u8; REPORT_BYTES]>,
    sent: Vec<[u8; REPORT_BYTES]>,
}

impl ScriptedLink {
    fn new(replies: VecDeque<[u8; REPORT_BYTES]>) -> Self {
        Self {
            replies,
            sent: Vec::new(),
        }
    }
}

impl AsRef<[[u8; REPORT_BYTES]]> for ScriptedLink {
    fn as_ref(&self) -> &[[u8; REPORT_BYTES]] {
        &self.sent
    }
}

impl ReportLink for ScriptedLink {
    fn send_report(&mut self, report: &[u8; REPORT_BYTES]) -> Result<(), crate::error::Error> {
        self.sent.push(*report);
        Ok(())
    }

    fn receive_report(&mut self) -> Result<[u8; REPORT_BYTES], crate::error::Error> {
        self.replies
            .pop_front()
            .ok_or_else(|| crate::error::Error::Protocol("scripted link ran out of replies".into()))
    }
}

#[test]
fn a_read_accepts_a_flagged_data_packet_size() {
    let mut replies = VecDeque::new();
    let mut ack = [0u8; REPORT_BYTES];
    ack[0] = 0xFF;
    ack[1] = 0x04;
    ack[2] = 0x80;
    replies.push_back(ack);

    let mut first = [0u8; REPORT_BYTES];
    first[0] = 0x82;
    first[1] = 0xBE;
    for (index, byte) in first[2..].iter_mut().enumerate() {
        *byte = index as u8;
    }
    replies.push_back(first);

    let mut second = [0u8; REPORT_BYTES];
    second[0] = 0x83;
    second[1] = 0xA9;
    for (index, byte) in second[2..2 + 41].iter_mut().enumerate() {
        *byte = 0xA0 + index as u8;
    }
    replies.push_back(second);

    let mut done = [0u8; REPORT_BYTES];
    done[0] = 0xFE;
    replies.push_back(done);

    let mut protocol = FileProtocol::new(ScriptedLink::new(replies));
    let data = protocol.read(0x06, 32).expect("flagged packet size");
    assert_eq!(data.len(), 124);
    assert_eq!(&data[..3], &[0, 1, 2]);
    assert_eq!(&data[62..65], &[0xA0, 0xA1, 0xA2]);
}

fn response(command: u8, sequence: u8) -> [u8; REPORT_BYTES] {
    let mut reply = [0u8; REPORT_BYTES];
    reply[0] = 0xFF;
    reply[1] = command;
    reply[2] = sequence;
    reply
}

fn open_response(handle: u8, size: u32) -> [u8; REPORT_BYTES] {
    let mut reply = response(0x01, 0);
    reply[4] = 0x01;
    reply[5] = handle;
    reply[6] = 0x04;
    reply[7..11].copy_from_slice(&size.to_be_bytes());
    reply
}

#[test]
fn ping_and_reset_send_exact_reports() {
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([response(0xFF, 1)])));
    protocol.ping().expect("ping");
    protocol.reset_filesystem().expect("reset");
    let sent = protocol.sent_reports();
    assert_eq!(sent.len(), 2);
    assert_eq!(&sent[0][..4], &[0xFF, 0x00, 0x00, 0x00]);
    assert_eq!(&sent[1][..6], &[0xFF, 0xFF, 0x01, 0x01, 0x01, 0x66]);
}

#[test]
fn command_reports_a_refusal() {
    let mut refusal = response(0xFF, 0);
    refusal[2] = 0xFF;
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([refusal])));
    let error = protocol.reset_filesystem().expect_err("refused");
    assert!(
        matches!(&error, Error::Protocol(message) if message.contains("refused command")),
        "{error}"
    );
}

#[test]
fn command_skips_stale_and_unsolicited_replies() {
    let stale = response(0x02, 0);
    let wrong_sequence = response(0xFF, 5);
    let good = response(0xFF, 0);
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([
        stale,
        wrong_sequence,
        good,
    ])));
    protocol
        .reset_filesystem()
        .expect("matched after stale replies");
    assert_eq!(protocol.sent_reports().len(), 1);
}

#[test]
fn command_gives_up_after_sixty_four_unmatched_replies() {
    let replies = (0..64).map(|_| response(0x02, 0)).collect();
    let mut protocol = FileProtocol::new(ScriptedLink::new(replies));
    let error = protocol.reset_filesystem().expect_err("no match");
    assert!(
        matches!(&error, Error::Protocol(message) if message.contains("no reply matching")),
        "{error}"
    );
}

#[test]
fn open_read_rejects_a_malformed_open_reply() {
    let mut malformed = response(0x01, 0);
    malformed[4] = 0;
    malformed[6] = 0;
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([malformed])));
    let error = protocol
        .open_read("/cfg/usercfg")
        .expect_err("malformed open");
    assert!(
        matches!(&error, Error::Protocol(message) if message.contains("open reply")),
        "{error}"
    );
}

#[test]
fn open_read_returns_the_handle_and_size() {
    let mut protocol =
        FileProtocol::new(ScriptedLink::new(VecDeque::from([open_response(3, 0x40)])));
    let opened = protocol.open_read("/cfg/usercfg").expect("open read");
    assert_eq!(opened.handle, 3);
    assert_eq!(opened.size, 0x40);
}

#[test]
fn open_write_sends_path_mode_and_size() {
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([open_response(
        7, 0x1234,
    )])));
    let handle = protocol
        .open_write("/cfg/usercfg", 0x1234)
        .expect("open write");
    assert_eq!(handle, 7);

    let request = protocol.sent_reports()[0];
    assert_eq!(request[1], 0x01);
    assert_eq!(request[2], 0x00);
    assert_eq!(request[3], 3);
    let path = b"/cfg/usercfg";
    assert_eq!(request[4], 0x80);
    assert_eq!(&request[5..5 + path.len()], path);
    let mut at = 5 + path.len();
    assert_eq!(request[at], 0);
    at += 1;
    assert_eq!(request[at], 0x80);
    assert_eq!(request[at + 1], b'W');
    assert_eq!(request[at + 2], 0);
    at += 3;
    assert_eq!(request[at], 0x04);
    assert_eq!(&request[at + 1..at + 5], &0x1234u32.to_be_bytes());
}

#[test]
fn read_refuses_a_remote_refusal() {
    let mut refusal = response(0x04, 0);
    refusal[2] = 0xFF;
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([refusal])));
    let error = protocol.read(6, 1).expect_err("refused read");
    assert!(
        matches!(&error, Error::Protocol(message) if message.contains("refused read")),
        "{error}"
    );
}

#[test]
fn read_skips_control_replies_and_stops_at_done() {
    let echo = response(0x04, 0);
    let mut packet = [0u8; REPORT_BYTES];
    packet[0] = 0x80;
    packet[1] = 1;
    packet[2] = 0x42;
    let mut done = [0u8; REPORT_BYTES];
    done[0] = 0xFE;
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([echo, packet, done])));
    let data = protocol.read(6, 3).expect("read");
    assert_eq!(data, [0x42]);
}

#[test]
fn write_chunk_rejects_empty_and_oversized_chunks() {
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::new()));
    let empty = protocol.write_chunk(1, &[], false).expect_err("empty");
    assert!(
        matches!(&empty, Error::Protocol(message) if message.contains("empty write chunk")),
        "{empty}"
    );
    let oversized = vec![0u8; DATA_PACKET_PAYLOAD * 255];
    let error = protocol
        .write_chunk(1, &oversized, false)
        .expect_err("oversized");
    assert!(
        matches!(&error, Error::Protocol(message) if message.contains("too many packets")),
        "{error}"
    );
}

#[test]
fn write_chunk_sends_payload_and_done_reports() {
    let reply = response(0x03, 0xFE);
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([reply])));
    protocol
        .write_chunk(4, &[7u8; DATA_PACKET_PAYLOAD], true)
        .expect("last chunk");
    let sent = protocol.sent_reports();
    assert_eq!(sent.len(), 3);
    assert_eq!(
        &sent[0][..8],
        &[0xFF, 0x03, 0x00, 0x02, 0x01, 0x04, 0x01, 0x03]
    );
    assert_eq!(&sent[1][..3], &[0x01, DATA_PACKET_PAYLOAD as u8, 0x07]);
    assert_eq!(sent[2][0], DONE_PACKET);
}

#[test]
fn write_chunk_rejects_an_unexpected_reply() {
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([[0u8; REPORT_BYTES]])));
    let error = protocol
        .write_chunk(4, &[1], false)
        .expect_err("unexpected");
    assert!(
        matches!(&error, Error::Protocol(message) if message.contains("unexpected write chunk reply")),
        "{error}"
    );
}

#[test]
fn check_reads_the_match_byte() {
    let mut matched = response(0x06, 0);
    matched[7] = b'm';
    let mut mismatched = response(0x06, 1);
    mismatched[7] = b'u';
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([matched, mismatched])));
    assert!(protocol.check(1, 0x1234, 10).expect("check"));
    assert!(!protocol.check(1, 0x1234, 10).expect("check"));
}

#[test]
fn flush_close_and_reboot_send_exact_reports() {
    let mut protocol = FileProtocol::new(ScriptedLink::new(VecDeque::from([
        response(0x05, 0),
        response(0x07, 1),
    ])));
    protocol.flush(9).expect("flush");
    protocol.close(9).expect("close");
    protocol.reboot().expect("reboot");
    let sent = protocol.sent_reports();
    assert_eq!(&sent[0][..6], &[0xFF, 0x05, 0x00, 0x02, 0x01, 0x09]);
    assert_eq!(&sent[0][6..8], &[0x01, 0x00]);
    assert_eq!(&sent[1][..6], &[0xFF, 0x07, 0x01, 0x01, 0x01, 0x09]);
    assert_eq!(&sent[2][..6], &[0xFF, 0xFF, 0x02, 0x01, 0x01, 0x00]);
}

#[test]
fn hidapi_path_names_the_usb_sysfs_device() {
    assert_eq!(crate::hid::sysfs_device("1-2:1.0"), Some("1-2"));
    assert_eq!(crate::hid::sysfs_device("1-2.3:1.0"), Some("1-2.3"));
    assert!(crate::hid::sysfs_device("/dev/hidraw0").is_none());
}
