extern crate serde_json;

use std::collections::HashMap;




use super::threadinfo::Thread;

/*
 * The below comment and struct definitons were copied from the speedscope sources in rbspy.
 * https://github.com/rbspy/rbspy/blob/d408b12dfc906292e1e85e6152a38416ed3a18e5/src/ui/speedscope.rs
 *
 * There are slight modifications to the SpeedscopeFile impl to support vignette.
 */

/*
 * This file contains code to export rbspy profiles for use in https://speedscope.app
 *
 * The TypeScript definitions that define this file format can be found here:
 * https://github.com/jlfwong/speedscope/blob/9d13d9/src/lib/file-format-spec.ts
 *
 * From the TypeScript definition, a JSON schema is generated. The latest
 * schema can be found here: https://speedscope.app/file-format-schema.json
 *
 * This JSON schema conveniently allows to generate type bindings for generating JSON.
 * You can use https://app.quicktype.io/ to generate serde_json Rust bindings for the
 * given JSON schema.
 *
 * There are multiple variants of the file format. The variant we're going to generate
 * is the "type: sampled" profile, since it most closely maps to rbspy's data recording
 * structure.
 */

#[derive(Debug, Serialize, Deserialize)]
pub struct SpeedscopeFile {
    #[serde(rename = "$schema")]
    schema: String,
    profiles: Vec<Profile>,
    shared: Shared,

    #[serde(rename = "activeProfileIndex")]
    active_profile_index: Option<f64>,

    exporter: Option<String>,

    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Profile {
    #[serde(rename = "type")]
    profile_type: ProfileType,

    name: String,
    unit: ValueUnit,

    #[serde(rename = "startValue")]
    start_value: f64,

    #[serde(rename = "endValue")]
    end_value: f64,

    samples: Vec<Vec<usize>>,
    weights: Vec<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Shared {
    frames: Vec<Frame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub name: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
enum ProfileType {
    #[serde(rename = "evented")]
    Evented,
    #[serde(rename = "sampled")]
    Sampled,
}

#[derive(Debug, Serialize, Deserialize)]
enum ValueUnit {
    #[serde(rename = "bytes")]
    Bytes,
    #[serde(rename = "microseconds")]
    Microseconds,
    #[serde(rename = "milliseconds")]
    Milliseconds,
    #[serde(rename = "nanoseconds")]
    Nanoseconds,
    #[serde(rename = "none")]
    None,
    #[serde(rename = "seconds")]
    Seconds,
}

impl SpeedscopeFile {
    pub fn new(
        samples: HashMap<Option<Thread>, Vec<Vec<usize>>>,
        frames: Vec<Frame>,
    ) -> SpeedscopeFile {
        let end_value = samples.len().clone();

        SpeedscopeFile {
            // This is always the same
            schema: "https://www.speedscope.app/file-format-schema.json".to_string(),

            active_profile_index: None,

            name: Some("vignette profile".to_string()),

            exporter: Some(format!("vignette@{}", env!("CARGO_PKG_VERSION"))),

            profiles: samples
                .iter()
                .map(|(option_pid, samples)| {
                    let weights: Vec<f64> = (&samples).into_iter().map(|_s| 1 as f64).collect();

                    return Profile {
                        profile_type: ProfileType::Sampled,

                        name: option_pid.map_or("vignette profile".to_string(), |pid| {
                            format!("vignette profile {:?}", pid)
                        }),

                        unit: ValueUnit::None,

                        start_value: 0.0,
                        end_value: end_value as f64,

                        samples: samples.clone(),
                        weights: weights,
                    };
                })
                .collect(),

            shared: Shared { frames: frames },
        }
    }
}
