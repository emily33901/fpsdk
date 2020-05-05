use std::collections::HashMap;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use bincode;
use log::{error, info, trace, LevelFilter};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use simple_logging;
#[cfg(unix)]
use simplelog::{ConfigBuilder, WriteLogger};

use fpsdk::host::{Event, GetName, Host, HostMessage};
use fpsdk::plugin::{self, Info, InfoBuilder, Plugin, StateReader, StateWriter};
use fpsdk::voice::{self, OutVoiceHandler, Voice, VoiceHandler};
use fpsdk::{create_plugin, AsRawPtr, MidiMessage, ProcessParamFlags, ValuePtr};

static ONCE: Once = Once::new();
const LOG_PATH: &str = "simple.log";

#[derive(Debug)]
struct Simple {
    host: Host,
    tag: plugin::Tag,
    param_names: Vec<String>,
    state: State,
    voice_handler: SimpleVoiceHandler,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    _time: u64,
    _param_1: f64,
    _param_2: i64,
}

impl Plugin for Simple {
    fn new(host: Host, tag: plugin::Tag) -> Self {
        init_log();

        info!("init plugin with tag {}", tag);

        Self {
            host,
            tag,
            param_names: vec![
                "Parameter 1".into(),
                "Parameter 2".into(),
                "Parameter 3".into(),
            ],
            state: Default::default(),
            voice_handler: Default::default(),
        }
    }

    fn info(&self) -> Info {
        info!("plugin {} will return info", self.tag);

        InfoBuilder::new_full_gen("Simple", "Simple", self.param_names.len() as u32)
            // InfoBuilder::new_effect("Simple", "Simple", self.param_names.len() as u32)
            // .want_new_tick()
            .with_out_voices(1)
            .build()
    }

    fn save_state(&mut self, writer: StateWriter) {
        let now = SystemTime::now();
        let time = now.duration_since(UNIX_EPOCH).expect("").as_secs();
        self.state._time = time;
        self.state._param_1 = time as f64 * 0.001;
        self.state._param_2 = time as i64 / 2;
        match bincode::serialize_into(writer, &self.state) {
            Ok(_) => info!("state {:?} saved", self.state),
            Err(e) => error!("error serializing state {}", e),
        }
    }

    fn load_state(&mut self, mut reader: StateReader) {
        let mut buf = [0; std::mem::size_of::<State>()];
        reader
            .read(&mut buf)
            .and_then(|_| {
                bincode::deserialize::<State>(&buf).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("error deserializing value {}", e),
                    )
                })
            })
            .and_then(|value| {
                self.state = value;
                Ok(info!("read state {:?}", self.state))
            })
            .unwrap_or_else(|e| error!("error reading value from state {}", e));
    }

    fn on_message(&mut self, message: HostMessage) -> Box<dyn AsRawPtr> {
        info!("{} got message from host: {:?}", self.tag, message);

        Box::new(0)
    }

    fn name_of(&self, message: GetName) -> String {
        info!("{} host asks name of {:?}", self.tag, message);

        match message {
            GetName::Param(index) => self.param_names[index].clone(),
            _ => "What?".into(),
        }
    }

    fn process_event(&mut self, event: Event) {
        info!("{} host sends event {:?}", self.tag, event);
    }

    fn tick(&mut self) {
        trace!("{} receive new tick", self.tag);
    }

    fn idle(&mut self) {
        trace!("{} idle", self.tag);
    }

    fn process_param(
        &mut self,
        index: usize,
        value: ValuePtr,
        flags: ProcessParamFlags,
    ) -> Box<dyn AsRawPtr> {
        info!(
            "{} process param: index {}, value {}, flags {:?}",
            self.tag,
            index,
            value.get::<i32>(),
            flags
        );
        Box::new(0)
    }

    fn midi_in(&mut self, message: MidiMessage) {
        trace!("receive MIDI message {:?}", message);
    }

    fn render(&mut self, input: &[[f32; 2]], output: &mut [[f32; 2]]) {
        if self.voice_handler.voices.len() < 1 {
            // consider it an effect
            input.iter().zip(output).for_each(|(inp, outp)| {
                outp[0] = inp[0] * 0.25;
                outp[1] = inp[1] * 0.25;
            });
        }
    }

    fn voice_handler(&mut self) -> &mut dyn VoiceHandler {
        &mut self.voice_handler
    }
}

#[derive(Debug, Default)]
struct SimpleVoiceHandler {
    voices: HashMap<voice::Tag, SimpleVoice>,
    out_handler: SimpleOutVoiceHandler,
}

impl VoiceHandler for SimpleVoiceHandler {
    fn trigger(&mut self, params: voice::Params, tag: voice::Tag) -> &mut dyn Voice {
        let voice = SimpleVoice::new(params, tag);
        trace!("trigger voice {:?}", voice);
        self.voices.insert(tag, voice);
        self.voices.get_mut(&tag).unwrap()
    }

    fn release(&mut self, tag: voice::Tag) {
        trace!("release voice {:?}", self.voices.get(&tag));
    }

    fn kill(&mut self, tag: voice::Tag) {
        trace!("host wants to kill voice with tag {}", tag);
        trace!("kill voice {:?}", self.voices.remove(&tag));
        trace!(
            "remaining voices count {}, {:?}",
            self.voices.len(),
            self.voices
        );
    }

    fn on_event(&mut self, tag: voice::Tag, event: voice::Event) -> Box<dyn AsRawPtr> {
        trace!("event {:?} for voice {:?}", event, self.voices.get(&tag));
        Box::new(0)
    }

    fn out_handler(&mut self) -> Option<&mut dyn OutVoiceHandler> {
        Some(&mut self.out_handler)
    }
}

#[derive(Debug)]
struct SimpleVoice {
    tag: voice::Tag,
    params: voice::Params,
}

impl SimpleVoice {
    pub fn new(params: voice::Params, tag: voice::Tag) -> Self {
        Self { tag, params }
    }
}

impl Voice for SimpleVoice {
    fn tag(&self) -> voice::Tag {
        self.tag
    }
}

#[derive(Debug, Default)]
struct SimpleOutVoiceHandler;

impl OutVoiceHandler for SimpleOutVoiceHandler {
    fn kill(&mut self, tag: voice::Tag) {
        trace!("kill out voice with tag {}", tag);
    }

    fn on_event(&mut self, tag: voice::Tag, event: voice::OutEvent) -> Box<dyn AsRawPtr> {
        trace!("event {:?} on out voice {}", event, tag);
        Box::new(0)
    }
}

fn init_log() {
    ONCE.call_once(|| {
        _init_log();
        info!("init log");
    });
}

#[cfg(windows)]
fn _init_log() {
    simple_logging::log_to_file(LOG_PATH, LevelFilter::Trace).unwrap();
}

#[cfg(unix)]
fn _init_log() {
    // the file is created at FL's resources root directory
    // for macOS it's /Applications/FL Studio 20.app/Contents/Resources/FL
    // for Windows it's <Drive>:\Program Files\Image-Line\FL Studio 20
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(LOG_PATH)
        .unwrap();
    let config = ConfigBuilder::new().set_time_to_local(true).build();
    let _ = WriteLogger::init(LevelFilter::Trace, config, file).unwrap();
}

create_plugin!(Simple);
