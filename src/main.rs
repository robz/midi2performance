use midly::{num::u7, MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use std::collections::HashSet;
use std::fs;
use tch::Tensor;

enum PerformanceEvent {
    NoteOn(u8),
    NoteOff(u8),
    TimeShift(u8),
    Velocity(u8),
}

fn event_to_string(event: &PerformanceEvent) -> String {
    return match event {
        PerformanceEvent::NoteOn(v) => format!("NoteOn {}", v),
        PerformanceEvent::NoteOff(v) => format!("NoteOff {}", v),
        PerformanceEvent::TimeShift(v) => format!("TimeShift {}", v),
        PerformanceEvent::Velocity(v) => format!("Velocity {}", v),
    };
}

fn event_to_index(event: &PerformanceEvent) -> i16 {
    return match event {
        PerformanceEvent::NoteOn(v) => *v as i16,
        PerformanceEvent::NoteOff(v) => *v as i16 + 128,
        PerformanceEvent::TimeShift(v) => *v as i16 + 256,
        PerformanceEvent::Velocity(v) => *v as i16 + 356,
    };
}

fn file_to_events(filename: &str) -> Vec<PerformanceEvent> {
    let data = std::fs::read(filename).unwrap();

    let smf = Smf::parse(&data).unwrap();

    let ticks_per_beat: u16 = match smf.header.timing {
        Timing::Metrical(x) => x,
        _ => panic!("oops"),
    }
    .into();
    let ticks_per_beat = ticks_per_beat as f64;
    let mut microsecs_per_beat = 0;
    for event in &smf.tracks[0] {
        match event.kind {
            TrackEventKind::Meta(MetaMessage::Tempo(x)) => {
                microsecs_per_beat = x.into();
            }
            _ => (),
        }
    }
    let millis_per_tick = (microsecs_per_beat as f64) / ticks_per_beat / 1e3;

    let mut is_pedal_down = false;
    let mut events: Vec<PerformanceEvent> = Vec::new();
    let mut sustained_notes: HashSet<u7> = HashSet::new();
    let mut notes_on: HashSet<u7> = HashSet::new();
    let mut previous_t = None;

    for event in &smf.tracks[1] {
        if event.delta > 0 {
            let ticks: u32 = event.delta.into();
            let ticks = ticks as f64;
            // event times are stored as ticks, so convert to milliseconds
            let ms = ticks * millis_per_tick;
            // combine repeated delta time events
            let mut t = ms
                + match previous_t {
                    Some(t) => t,
                    None => 0.0,
                };

            // we can only represent a max time of 1 second (1000 ms)
            // so we must split up times that are larger than that
            // into separate events
            let mut t_chunk = 0.0;
            while t > 0.0 {
                t_chunk = if t > 1000.0 { 1000.0 } else { t };
                let event = PerformanceEvent::TimeShift((t_chunk / 10.0).ceil() as u8 - 1);
                if previous_t == None {
                    events.push(event);
                } else {
                    // update the last time event to combine timeshifts
                    *(events.last_mut().unwrap()) = event;
                    previous_t = None;
                }
                t -= t_chunk;
            }
            previous_t = Some(t_chunk);
        }
        match event.kind {
            TrackEventKind::Midi {
                channel: _,
                message,
            } => match message {
                MidiMessage::NoteOn { key, vel } => {
                    if vel == 0 {
                        if is_pedal_down {
                            sustained_notes.insert(key);
                        } else {
                            events.push(PerformanceEvent::NoteOff(key.into()));
                            previous_t = None;
                            notes_on.remove(&key);
                        }
                    } else {
                        if sustained_notes.contains(&key) {
                            events.push(PerformanceEvent::NoteOff(key.into()));
                            sustained_notes.remove(&key);
                            notes_on.remove(&key);
                        }
                        let vel: u8 = vel.into();
                        events.push(PerformanceEvent::Velocity(vel / 4));
                        events.push(PerformanceEvent::NoteOn(key.into()));
                        previous_t = None;
                        notes_on.insert(key);
                    }
                }
                MidiMessage::Controller { controller, value } if controller == 64 => {
                    if is_pedal_down && value < 64 {
                        for key in &sustained_notes {
                            events.push(PerformanceEvent::NoteOff((*key).into()));
                            notes_on.remove(&key);
                        }
                        if sustained_notes.len() > 0 {
                            previous_t = None;
                        }
                        sustained_notes.clear();
                    }
                    is_pedal_down = value >= 64;
                }
                _ => {}
            },
            _ => {}
        }
    }

    return events;
}

fn convert_maestro(input_path: &str, output_path: &str) -> std::result::Result<(), std::io::Error> {
    if !std::path::Path::new(output_path).is_dir() {
        fs::create_dir(output_path)?;
    }
    for entry in fs::read_dir(input_path)? {
        let entry = entry?;
        let path = entry.path();
        println!("processing {:?}", path.file_name().unwrap());
        let metadata = fs::metadata(&path)?;
        if metadata.is_file() {
            continue;
        }
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let filename = path.file_name().unwrap();
            let path = entry.path();
            let filepath = path.into_os_string().into_string().unwrap();
            let events = file_to_events(&filepath);
            let events: Vec<i16> = events.into_iter().map(|x| event_to_index(&x)).collect();
            Tensor::of_slice(&events)
                .save(format!("{}/{:?}.pt", output_path, filename))
                .unwrap();
        }
    }
    return Ok(());
}

fn main() {
    convert_maestro("maestro-v3.0.0", "tensors").unwrap();
}
