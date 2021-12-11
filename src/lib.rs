use midly::{num::u7, MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use std::collections::HashSet;

#[derive(Debug)]
pub enum PerformanceEvent {
    NoteOn(i16),
    NoteOff(i16),
    TimeShift(i16),
    Velocity(i16),
}

fn u7_to_i16(v: &u7) -> i16 {
    let v: u8 = (*v).into();
    v as i16
}

pub fn event_to_index(event: PerformanceEvent) -> i16 {
    match event {
        PerformanceEvent::NoteOn(v) => v,
        PerformanceEvent::NoteOff(v) => v + 128,
        PerformanceEvent::TimeShift(v) => v + 256,
        PerformanceEvent::Velocity(v) => v / 4 + 356,
    }
}

#[allow(dead_code)]
pub fn index_to_event(idx: i16) -> Result<PerformanceEvent, String> {
    if idx >= 0 && idx < 128 {
        Ok(PerformanceEvent::NoteOn(idx))
    } else if idx >= 128 && idx < 256 {
        Ok(PerformanceEvent::NoteOff(idx - 128))
    } else if idx >= 256 && idx < 356 {
        Ok(PerformanceEvent::TimeShift(idx - 256))
    } else if idx >= 356 && idx < 388 {
        Ok(PerformanceEvent::Velocity(idx - 356))
    } else {
        Err(String::from(format!("index {} not supported", idx)))
    }
}

pub fn midi_to_events(smf: &Smf) -> Vec<PerformanceEvent> {
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
    let mut sustained_notes: HashSet<i16> = HashSet::new();
    let mut notes_on: HashSet<i16> = HashSet::new();
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
                let event = PerformanceEvent::TimeShift(time_value as i16);
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
                    let key = u7_to_i16(&key);
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
                        events.push(PerformanceEvent::Velocity(u7_to_i16(&vel)));
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
