mod mapping;

pub use mapping::{MidiAction, MidiBindingKey, MidiMap, MidiMessageType, TriggerMode};

use crossbeam_channel::{Receiver, Sender};

pub struct MidiDeviceInfo {
    pub name: String,
    pub port_index: usize,
}

pub struct MidiInput {
    _conn: Option<midir::MidiInputConnection<()>>,
    rx: Receiver<MidiMessage>,
}

#[derive(Debug, Clone)]
pub struct MidiMessage {
    pub channel: u8,
    pub message_type: MidiMessageType,
    pub data_byte: u8,
    pub value: u8,
}

impl MidiInput {
    pub fn new() -> Self {
        let (_tx, rx): (Sender<MidiMessage>, Receiver<MidiMessage>) =
            crossbeam_channel::bounded(256);
        Self { _conn: None, rx }
    }

    pub fn enumerate_devices() -> Vec<MidiDeviceInfo> {
        match midir::MidiInput::new("ToneDock MIDI scan") {
            Ok(midi_in) => midi_in
                .ports()
                .iter()
                .enumerate()
                .filter_map(|(i, port)| {
                    midi_in.port_name(port).ok().map(|name| MidiDeviceInfo {
                        name,
                        port_index: i,
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn open_device(&mut self, port_index: usize) -> Result<(), String> {
        let midi_in =
            midir::MidiInput::new("ToneDock").map_err(|e| format!("MIDI init error: {}", e))?;

        let ports = midi_in.ports();
        let port = ports
            .get(port_index)
            .ok_or_else(|| format!("MIDI port {} not found", port_index))?;

        let (tx, rx_new): (Sender<MidiMessage>, Receiver<MidiMessage>) =
            crossbeam_channel::bounded(256);

        let conn = midi_in
            .connect(
                port,
                "ToneDock MIDI input",
                move |_stamp, data, _| {
                    if data.len() < 2 {
                        return;
                    }
                    let status = data[0];
                    let channel = status & 0x0F;
                    let msg_type = status & 0xF0;

                    let parsed = match msg_type {
                        0x90 if data.len() >= 3 && data[2] > 0 => Some(MidiMessage {
                            channel,
                            message_type: MidiMessageType::NoteOn,
                            data_byte: data[1],
                            value: data[2],
                        }),
                        0x80 | 0x90 => Some(MidiMessage {
                            channel,
                            message_type: MidiMessageType::NoteOff,
                            data_byte: data[1],
                            value: 0,
                        }),
                        0xB0 if data.len() >= 3 => Some(MidiMessage {
                            channel,
                            message_type: MidiMessageType::ControlChange,
                            data_byte: data[1],
                            value: data[2],
                        }),
                        0xC0 => Some(MidiMessage {
                            channel,
                            message_type: MidiMessageType::ProgramChange,
                            data_byte: data[1] & 0x7F,
                            value: 0,
                        }),
                        _ => None,
                    };

                    if let Some(msg) = parsed {
                        let _ = tx.try_send(msg);
                    }
                },
                (),
            )
            .map_err(|e| format!("MIDI connect error: {}", e))?;

        self._conn = Some(conn);
        self.rx = rx_new;
        Ok(())
    }

    pub fn close(&mut self) {
        self._conn = None;
    }

    pub fn try_recv_messages(&self) -> Vec<MidiMessage> {
        let mut msgs = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            msgs.push(msg);
        }
        msgs
    }

    pub fn is_connected(&self) -> bool {
        self._conn.is_some()
    }
}
