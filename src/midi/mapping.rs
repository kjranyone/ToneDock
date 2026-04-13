use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidiAction {
    PresetUp,
    PresetDown,
    LooperRecord,
    LooperStop,
    LooperPlay,
    LooperOverdub,
    LooperClear,
    LooperUndo,
    TapTempo,
    BackingPlay,
    BackingStop,
    MetronomeToggle,
    PanicMute,
    MasterVolumeUp,
    MasterVolumeDown,
    ToggleBypassSelected,
}

impl MidiAction {
    pub fn label(&self) -> &'static str {
        match self {
            MidiAction::PresetUp => "Preset Up",
            MidiAction::PresetDown => "Preset Down",
            MidiAction::LooperRecord => "Looper Record",
            MidiAction::LooperStop => "Looper Stop",
            MidiAction::LooperPlay => "Looper Play",
            MidiAction::LooperOverdub => "Looper Overdub",
            MidiAction::LooperClear => "Looper Clear",
            MidiAction::LooperUndo => "Looper Undo",
            MidiAction::TapTempo => "Tap Tempo",
            MidiAction::BackingPlay => "Backing Play",
            MidiAction::BackingStop => "Backing Stop",
            MidiAction::MetronomeToggle => "Metronome Toggle",
            MidiAction::PanicMute => "Panic Mute",
            MidiAction::MasterVolumeUp => "Master Volume Up",
            MidiAction::MasterVolumeDown => "Master Volume Down",
            MidiAction::ToggleBypassSelected => "Toggle Bypass (Selected)",
        }
    }

    pub fn all() -> &'static [MidiAction] {
        &[
            MidiAction::PresetUp,
            MidiAction::PresetDown,
            MidiAction::LooperRecord,
            MidiAction::LooperStop,
            MidiAction::LooperPlay,
            MidiAction::LooperOverdub,
            MidiAction::LooperClear,
            MidiAction::LooperUndo,
            MidiAction::TapTempo,
            MidiAction::BackingPlay,
            MidiAction::BackingStop,
            MidiAction::MetronomeToggle,
            MidiAction::PanicMute,
            MidiAction::MasterVolumeUp,
            MidiAction::MasterVolumeDown,
            MidiAction::ToggleBypassSelected,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiBindingKey {
    pub channel: u8,
    pub message_type: MidiMessageType,
    pub data_byte: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidiMessageType {
    NoteOn,
    NoteOff,
    ControlChange,
    ProgramChange,
}

impl MidiBindingKey {
    pub fn display(&self) -> String {
        let type_str = match self.message_type {
            MidiMessageType::NoteOn => "NoteOn",
            MidiMessageType::NoteOff => "NoteOff",
            MidiMessageType::ControlChange => "CC",
            MidiMessageType::ProgramChange => "PC",
        };
        match self.message_type {
            MidiMessageType::NoteOn | MidiMessageType::NoteOff => {
                let note_name = match self.data_byte % 12 {
                    0 => "C",
                    1 => "C#",
                    2 => "D",
                    3 => "D#",
                    4 => "E",
                    5 => "F",
                    6 => "F#",
                    7 => "G",
                    8 => "G#",
                    9 => "A",
                    10 => "A#",
                    11 => "B",
                    _ => "?",
                };
                let octave = (self.data_byte / 12) as i32 - 1;
                format!(
                    "{} {}{} (ch{})",
                    type_str,
                    note_name,
                    octave,
                    self.channel + 1
                )
            }
            MidiMessageType::ControlChange => {
                format!("{} #{} (ch{})", type_str, self.data_byte, self.channel + 1)
            }
            MidiMessageType::ProgramChange => {
                format!("{} #{} (ch{})", type_str, self.data_byte, self.channel + 1)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiBinding {
    pub key: MidiBindingKey,
    pub action: MidiAction,
    pub mode: TriggerMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    Toggle,
    Momentary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiMap {
    pub name: String,
    pub bindings: Vec<MidiBinding>,
}

impl Default for MidiMap {
    fn default() -> Self {
        Self {
            name: "Default".into(),
            bindings: Vec::new(),
        }
    }
}

impl MidiMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_action(&self, key: &MidiBindingKey) -> Option<(MidiAction, TriggerMode)> {
        self.bindings
            .iter()
            .find(|b| b.key == *key)
            .map(|b| (b.action, b.mode))
    }

    pub fn find_binding(&self, action: MidiAction) -> Option<&MidiBinding> {
        self.bindings.iter().find(|b| b.action == action)
    }

    pub fn set_binding(&mut self, key: MidiBindingKey, action: MidiAction, mode: TriggerMode) {
        self.bindings.retain(|b| b.action != action && b.key != key);
        self.bindings.push(MidiBinding { key, action, mode });
    }

    pub fn remove_binding_for_action(&mut self, action: MidiAction) {
        self.bindings.retain(|b| b.action != action);
    }

    pub fn clear(&mut self) {
        self.bindings.clear();
    }
}
