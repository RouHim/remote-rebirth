//! What the window can be told to do, and the chapters it can show.
//!
//! Every user action crosses the UI boundary as one [`Message`]. The enums here are the vocabulary
//! of the window; the work they trigger lives in [`crate::handlers`].

use std::path::PathBuf;

use iced::keyboard::{Key, Modifiers};
use remote_rebirth_core::Progress;
use remote_rebirth_core::gui_api::{BackupResult, ComposeResult, DetectReport, WriteResult};

/// The seven chapters of the manual, in display order.
///
/// This is the one ordered list: the stored page index, the Ctrl+1…7 shortcuts and the
/// Ctrl+PageUp/PageDown cycle all derive from it.
pub(crate) const PAGES: [Page; 7] = [
    Page::Remote,
    Page::Devices,
    Page::KeyMap,
    Page::Activities,
    Page::Favourites,
    Page::Library,
    Page::Settings,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedKind {
    Write,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Detect,
    Backup,
    Prepare,
    Commit,
    ReadBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Remote,
    Devices,
    KeyMap,
    Activities,
    Favourites,
    Library,
    Settings,
}

impl Page {
    /// Position in the chapter order, which is also the stored `GuiState::page`.
    pub(crate) fn index(self) -> u8 {
        PAGES
            .iter()
            .position(|page| *page == self)
            .unwrap_or_default() as u8
    }

    /// Chapter for a stored index; unknown indices fall back to the first chapter.
    pub(crate) fn from_index(index: u8) -> Self {
        PAGES
            .get(usize::from(index))
            .copied()
            .unwrap_or(Page::Remote)
    }

    /// The chapter `delta` steps away, wrapping around the chapter order.
    pub(crate) fn cycle(self, delta: isize) -> Self {
        let length = PAGES.len() as isize;
        let next = (isize::from(self.index()) + delta).rem_euclid(length) as usize;
        PAGES[next]
    }
}

/// Chapter for a single Ctrl+digit shortcut, `1` being the first chapter.
pub(crate) fn page_for_digit(text: &str) -> Option<Page> {
    let mut chars = text.chars();
    let digit = chars.next()?.to_digit(10)?;
    if chars.next().is_some() || digit == 0 {
        return None;
    }
    PAGES.get(digit as usize - 1).copied()
}

#[derive(Debug, Clone)]
pub(crate) enum OpDone {
    Detected(DetectReport),
    BackedUp(BackupResult),
    Composed(ComposeResult),
    Prepared(WriteResult),
    Committed(WriteResult),
    RestorePrepared(WriteResult),
    Restored(WriteResult),
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Page(Page),
    Shortcut(Key, Modifiers),
    Detect,
    Backup,
    Compose,
    PrepareWrite,
    CommitWrite,
    PrepareRestore,
    CommitRestore,
    Progress(Progress),
    Done(Result<OpDone, String>),
    Cancel,
    SelectDevice(usize),
    NewDeviceName(String),
    NewDeviceKey(u8),
    AddDevice,
    DeviceName(usize, String),
    DeviceKey(usize, u8),
    DeleteDevice(usize),
    SelectKey(String),
    ClearKey,
    CommandFilter(String),
    AssignCommand(String),
    AssignFirstCommand,
    NewCommandName(String),
    NewCommandFormat(String),
    NewCommandFile(String),
    NewCommandRemote(String),
    NewCommandButton(String),
    NewCommandHex(String),
    AddCommand,
    DeleteCommand(String),
    RoutingVolume(Option<String>),
    RoutingChannel(Option<String>),
    SkinChanged(u8),
    UseDetectedSkin(u8),
    Reboot(bool),
    ToggleAdvanced(bool),
    ActivityChannel(String),
    ActivityDevice(String),
    ActivityPower(String),
    ActivityInput(String),
    ActivityDelay(String),
    StepDevice(usize, String),
    StepPower(usize, String),
    StepInput(usize, String),
    StepDelay(usize, String),
    AddStep,
    DeleteStep(usize),
    NewFavouriteChannel(String),
    AddFavourite,
    FavouriteChannel(usize, String),
    DeleteFavourite(usize),
    LibraryPath(String),
    ScanLibrary,
    LibrarySearch(String),
    SelectLibraryDevice(usize),
    AddLibraryDevice,
    RestorePath(String),
    Save,
    Reload,
    TransferPath(String),
    ExportModel,
    ImportModel,
    UseBackup(PathBuf),
    RefreshBackups,
    DeleteBackup(PathBuf),
    DismissError,
}

/// One device found by a local IR Library scan.
#[derive(Debug, Clone)]
pub(crate) struct LibraryDevice {
    pub(crate) brand: String,
    pub(crate) model: String,
    pub(crate) path: PathBuf,
    pub(crate) commands: Vec<String>,
}

/// The in-progress text of the "Add a command" form.
#[derive(Debug, Clone)]
pub(crate) struct CommandForm {
    pub(crate) name: String,
    pub(crate) format: String,
    pub(crate) file: String,
    pub(crate) remote: String,
    pub(crate) button: String,
    pub(crate) hex: String,
}

impl Default for CommandForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            format: "pronto".into(),
            file: String::new(),
            remote: String::new(),
            button: String::new(),
            hex: String::new(),
        }
    }
}
