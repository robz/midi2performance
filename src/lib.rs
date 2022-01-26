use midly::{num::u7, Format, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};
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

#[allow(dead_code)]
fn timeshift_to_ms(timeshift: i16) -> i16 {
    // timeshifts are discretized 10 ms chunks, starting at 0
    (timeshift + 1) * 10
}

fn ticks_to_timeshift(ticks: u32, ticks_per_sec: u32) -> u32 {
    // timeshifts are discretized 10 ms chunks, starting at 0
    (ticks * 100 - 50) / ticks_per_sec
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

fn merge_parallel_tracks<'a>(tracks: &Vec<Vec<TrackEvent<'a>>>) -> Vec<TrackEvent<'a>> {
    let mut combined_track = vec![];
    for track in tracks {
        let mut t = 0u32;
        for event in track {
            let delta: u32 = event.delta.into();
            t += delta;
            combined_track.push((event, t));
        }
    }
    combined_track.sort_by_key(|v| v.1);
    let mut prev_t = 0u32;
    let mut track: Vec<TrackEvent> = vec![];
    for (event, new_t) in combined_track {
        track.push(TrackEvent {
            delta: (new_t - prev_t).into(),
            kind: event.kind,
        });
        prev_t = new_t;
    }
    return track;
}

#[cfg(test)]
mod tests {
    use rand::Rng;
    use super::*;

    fn create_random_midievent() -> TrackEvent<'static> {
        let channel = rand::thread_rng().gen_range(0..16);
        let delta = rand::thread_rng().gen_range(0..100);
        let key = rand::thread_rng().gen_range(0..128);
        let vel = rand::thread_rng().gen_range(0..128);
        let note_on = rand::thread_rng().gen_bool(0.5);
        return TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Midi {
                channel: channel.into(),
                message: if note_on {
                    MidiMessage::NoteOn {
                        key: key.into(),
                        vel: vel.into(),
                    }
                } else {
                    MidiMessage::NoteOff {
                        key: key.into(),
                        vel: vel.into(),
                    }
                },
            },
        };
    }

    #[test]
    fn single_track_remains_the_same() {
        let mut tracks = vec![vec![]];
        assert_eq!(merge_parallel_tracks(&tracks), tracks[0]);

        tracks[0].push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(b"violin")),
        });
        assert_eq!(merge_parallel_tracks(&tracks), tracks[0]);

        for _ in 0..100 {
            tracks[0].push(create_random_midievent());
        }
        assert_eq!(merge_parallel_tracks(&tracks), tracks[0]);
    }

    #[test]
    fn two_tracks_are_concated() {
        let tracks = vec![
            vec![TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(b"violin")),
            }],
            (0..100).map(|_| create_random_midievent()).collect(),
        ];
        assert_eq!(
            merge_parallel_tracks(&tracks),
            tracks[0]
                .iter()
                .cloned()
                .chain(tracks[1].iter().cloned())
                .collect::<Vec<TrackEvent>>()
        );
    }

    #[test]
    fn multi_tracks_are_combined() {
        let track0: Vec<TrackEvent> = (0..2).map(|_| create_random_midievent()).collect();
        let track1: Vec<TrackEvent> = (0..2).map(|_| create_random_midievent()).collect();
        let track0_delta = track0.iter().fold(0u32, |acc, event: &TrackEvent| {
            let delta: u32 = event.delta.into();
            acc + delta
        });

        let mut track1_copy = track1.clone();
        let delta: u32 = track1_copy[0].delta.into();
        track1_copy[0].delta = (delta + track0_delta).into();

        let tracks = vec![track0, track1_copy];
        assert_eq!(
            merge_parallel_tracks(&tracks),
            tracks[0]
                .iter()
                .cloned()
                .chain(track1.iter().cloned())
                .collect::<Vec<TrackEvent>>()
        );
    }
}

fn get_tracks<'a>(smf: &mut Smf<'a>) -> Vec<TrackEvent<'a>> {
    return match smf.header.format {
        Format::SingleTrack => smf.tracks.remove(0),
        Format::Parallel => merge_parallel_tracks(&smf.tracks),
        Format::Sequential => panic!("Sequential tracks not supported"),
    };
}

pub fn midi_to_events(smf: &mut Smf) -> Vec<PerformanceEvent> {
    let ticks_per_beat: u16 = match smf.header.timing {
        Timing::Metrical(x) => x.into(),
        _ => panic!("Could not find metric timing header"),
    };
    let tracks = get_tracks(smf);
    let mut us_per_beat: Option<u32> = None;
    for event in &tracks {
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
    let mut previous_ticks = 0;

    for event in tracks {
        if event.delta > 0 {
            let mut ticks: u32 = event.delta.into();
            // combine repeated delta time events
            ticks += previous_ticks;

            // split up times that are larger than the max time into separate events
            let mut ticks_chunk = 0;
            while ticks > 0 {
                ticks_chunk = if ticks > ticks_per_sec { ticks_per_sec } else { ticks };
                let timeshift = ticks_to_timeshift(ticks_chunk, ticks_per_sec);
                let time_event = PerformanceEvent::TimeShift(timeshift as i16);
                if previous_ticks == 0 {
                    events.push(time_event);
                } else {
                    // update the last time event to combine timeshifts
                    let last_event_idx = events.len() - 1;
                    events[last_event_idx] = time_event;
                    previous_ticks = 0;
                }
                ticks -= ticks_chunk;
            }
            // record ticks from the last chunk
            previous_ticks = ticks_chunk;
        }

        let events_len_start = events.len();
        match event.kind {
            TrackEventKind::Midi {
                channel: _,
                message,
            } => match message {
                MidiMessage::NoteOn { key, vel } => {
                    let key = u7_to_i16(&key);
                    if vel == 0 {
                        if is_pedal_down && notes_on.contains(&key) {
                            sustained_notes.insert(key);
                        } else if notes_on.remove(&key) {
                            events.push(PerformanceEvent::NoteOff(key));
                        }
                    } else {
                        if sustained_notes.remove(&key) {
                            events.push(PerformanceEvent::NoteOff(key));
                            events.push(PerformanceEvent::Velocity(u7_to_i16(&vel)));
                            events.push(PerformanceEvent::NoteOn(key));
                        } else if notes_on.insert(key) {
                            events.push(PerformanceEvent::Velocity(u7_to_i16(&vel)));
                            events.push(PerformanceEvent::NoteOn(key));
                        }
                    }
                }
                MidiMessage::Controller { controller, value } if controller == 64 => {
                    if is_pedal_down && value < 64 {
                        for &key in &sustained_notes {
                            if notes_on.remove(&key) {
                                events.push(PerformanceEvent::NoteOff(key));
                            }
                        }
                        sustained_notes.clear();
                    }
                    is_pedal_down = value >= 64;
                }
                _ => {}
            },
            _ => {}
        }
        if events.len() > events_len_start {
            previous_ticks = 0;
        }
    }

    events
}
