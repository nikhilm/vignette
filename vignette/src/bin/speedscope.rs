// convert a resolved profile to a speedscope profile.
extern crate serde_json;
extern crate vignette;

use std::collections::HashMap;
use vignette::output;
use vignette::speedscope;

fn main() {
    let resolved_profile_path = std::env::args().nth(1).expect("profile path");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(resolved_profile_path)
        .expect("file");
    let resolved_profile: output::ResolvedProfile = serde_json::from_reader(file).unwrap();

    let speed_frames: Vec<speedscope::Frame> = resolved_profile
        .frames
        .iter()
        .map(|frame| speedscope::Frame {
            name: frame.name.clone(),
            file: Some(frame.file.clone()),
            line: Some(frame.line),
            col: None,
        })
        .collect();

    let speed_samples: HashMap<Option<_>, Vec<Vec<usize>>> = resolved_profile
        .threads
        .into_iter()
        .map(|thread| {
            let samples: Vec<Vec<usize>> = thread
                .samples
                .into_iter()
                .map(|mut sample| {
                    sample.frames.reverse();
                    sample.frames
                })
                .collect();
            (Some(thread.thread_id), samples)
        })
        .collect();

    let speed = speedscope::SpeedscopeFile::new(speed_samples, speed_frames);
    let stdout = std::io::stdout();
    serde_json::to_writer_pretty(stdout.lock(), &speed);
}
