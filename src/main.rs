use midly::{num::u7, MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use std::{collections::HashSet, env, fs, io::Error, path::Path};
use tch::Tensor;

#[derive(Debug)]
enum PerformanceEvent {
    NoteOn(u7),
    NoteOff(u7),
    TimeShift(u8),
    Velocity(u7),
}

fn u7_to_i16(v: &u7) -> i16 {
    let v: u8 = (*v).into();
    v as i16
}

fn event_to_index(event: &PerformanceEvent) -> i16 {
    match event {
        PerformanceEvent::NoteOn(v) => u7_to_i16(v),
        PerformanceEvent::NoteOff(v) => u7_to_i16(v) + 128,
        PerformanceEvent::TimeShift(v) => *v as i16 + 256,
        PerformanceEvent::Velocity(v) => u7_to_i16(v) / 4 + 356,
    }
}

fn midi_to_events(smf: &Smf) -> Vec<PerformanceEvent> {
    let ticks_per_beat: u16 = match smf.header.timing {
        Timing::Metrical(x) => x.into(),
        _ => panic!("Could not find metric timing header"),
    };
    let mut us_per_beat: Option<u32> = None;
    for event in &smf.tracks[0] {
        match event.kind {
            TrackEventKind::Meta(MetaMessage::Tempo(x)) => {
                us_per_beat = Some(x.into());
                break;
            }
            _ => (),
        }
    }
    let us_per_beat = us_per_beat.expect("Could not find tempo message");
    let ticks_per_sec = (ticks_per_beat as u32) * 1_000_000 / us_per_beat;

    let mut is_pedal_down = false;
    let mut events: Vec<PerformanceEvent> = Vec::new();
    let mut sustained_notes: HashSet<u7> = HashSet::new();
    let mut notes_on: HashSet<u7> = HashSet::new();
    let mut previous_t = None;

    for event in &smf.tracks[1] {
        if event.delta > 0 {
            let ticks: u32 = event.delta.into();
            // combine repeated delta time events
            let mut t = ticks + previous_t.unwrap_or(0);

            // split up times that are larger than the max time into separate events
            let mut t_chunk = 0;
            while t > 0 {
                t_chunk = if t > ticks_per_sec { ticks_per_sec } else { t };
                // time values are discretized 10 ms chunks, starting at 0
                let time_value = (t_chunk * 100 - 50) / ticks_per_sec;
                let event = PerformanceEvent::TimeShift(time_value as u8);
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
                            events.push(PerformanceEvent::NoteOff(key));
                            previous_t = None;
                            notes_on.remove(&key);
                        }
                    } else {
                        if sustained_notes.contains(&key) {
                            events.push(PerformanceEvent::NoteOff(key));
                            sustained_notes.remove(&key);
                            notes_on.remove(&key);
                        }
                        events.push(PerformanceEvent::Velocity(vel));
                        events.push(PerformanceEvent::NoteOn(key));
                        previous_t = None;
                        notes_on.insert(key);
                    }
                }
                MidiMessage::Controller { controller, value } if controller == 64 => {
                    if is_pedal_down && value < 64 {
                        for &key in &sustained_notes {
                            events.push(PerformanceEvent::NoteOff(key));
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

    events
}

fn convert_directory_recursively(input_path: &str, output_path: &str) -> Result<(), Error> {
    if !Path::new(output_path).is_dir() {
        fs::create_dir_all(&output_path).expect(&format!(
            "could not create output directory '{}'",
            output_path
        ));
    }
    for entry in fs::read_dir(&input_path)
        .expect(&format!("could not read input directory '{}'", input_path))
    {
        let path = entry?.path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if path.metadata()?.is_dir() {
            println!("processing {}...", name);
            let output_subdir = format!("{}/{}", output_path, name);
            convert_directory_recursively(path.to_str().unwrap(), &output_subdir)?;
            continue;
        }
        let data = fs::read(&path).expect(&format!("Could not read file {:?}", path));
        let smf = match Smf::parse(&data) {
            Ok(smf) => smf,
            Err(error) => {
                println!(
                    "Failed to parse file {:?} due to midly error: {}",
                    path, error
                );
                continue;
            }
        };
        let events: Vec<i16> = midi_to_events(&smf)
            .into_iter()
            .map(|x| event_to_index(&x))
            .collect();
        let output_name = format!("{}/{}.pt", output_path, name);
        println!("{}", output_name);
        Tensor::of_slice(&events)
            .save(output_name)
            .expect("unable to save events to pytorch file");
    }
    return Ok(());
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let input_path = &args[1];
    let output_path = &args[2];
    convert_directory_recursively(input_path, output_path)?;
    println!("done!");
    return Ok(());
}
