//! What the dialog may ask of the tracer, and how that becomes a vtracer [`Config`].
//!
//! A preset first, then the few dials the dialog exposes, each clamped here: this is a
//! boundary — the JSON comes from a page — and an out-of-range value must become the
//! nearest sane one, not a panic inside clustering.

use serde::Deserialize;
use vtracer::{Clustering, Config, FitMode, Hierarchical, Preset};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PresetName {
    /// Black-and-white line art: threshold, then trace the dark parts.
    Bw,
    /// Flat colour with a full palette.
    Poster,
    /// Photographs: heavier speckle filter, coarser layers, rounder corners.
    Photo,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TraceMode {
    Pixel,
    Polygon,
    Spline,
}

/// One trace request, as the dialog sends it. Absent dials keep the preset's value.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceConfig {
    pub preset: PresetName,
    /// Significant bits per channel, 1..=8: fewer merges more colours.
    #[serde(default)]
    pub color_precision: Option<i32>,
    /// Colour step between gradient layers, 0..=255: larger is fewer layers.
    #[serde(default)]
    pub layer_difference: Option<i32>,
    /// Side of the smallest patch kept, in pixels (its area is the square).
    #[serde(default)]
    pub filter_speckle: Option<usize>,
    /// Degrees of turn that count as a corner, 0..=180. Higher is smoother.
    #[serde(default)]
    pub corner_threshold: Option<i32>,
    #[serde(default)]
    pub mode: Option<TraceMode>,
}

impl TraceConfig {
    /// The vtracer configuration this asks for.
    ///
    /// Always [`Hierarchical::Stacked`]: the editable result is one element per region,
    /// and a stacked trace is the one whose regions still read correctly when each is
    /// painted whole on top of the one below. The mosaic's shared boundaries would have
    /// to be split back apart to become elements at all.
    pub fn to_vtracer(&self) -> Config {
        let mut config = Config::from_preset(match self.preset {
            PresetName::Bw => Preset::Bw,
            PresetName::Poster => Preset::Poster,
            PresetName::Photo => Preset::Photo,
        });
        config.hierarchical = Hierarchical::Stacked;
        if let Some(bits) = self.color_precision {
            config.color_precision = bits.clamp(1, 8);
        }
        if let Some(step) = self.layer_difference {
            config.layer_difference = step.clamp(0, 255);
        }
        if let Some(side) = self.filter_speckle {
            config.filter_speckle = side.min(128);
        }
        if let Some(degrees) = self.corner_threshold {
            config.corner_threshold = degrees.clamp(0, 180);
        }
        if let Some(mode) = self.mode {
            config.mode = match mode {
                TraceMode::Pixel => FitMode::Pixel,
                TraceMode::Polygon => FitMode::Polygon,
                TraceMode::Spline => FitMode::Spline,
            };
        }
        debug_assert!(
            config.clustering != Clustering::Watershed,
            "no preset the dialog offers clusters by watershed"
        );
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> TraceConfig {
        serde_json::from_str(json).expect("valid config")
    }

    #[test]
    fn a_preset_alone_is_the_preset_stacked() {
        let config = parse(r#"{"preset":"photo"}"#).to_vtracer();
        let photo = Config::from_preset(Preset::Photo);
        assert_eq!(config.filter_speckle, photo.filter_speckle);
        assert_eq!(config.layer_difference, photo.layer_difference);
        assert_eq!(config.corner_threshold, photo.corner_threshold);
        assert_eq!(config.hierarchical, Hierarchical::Stacked);
        assert_eq!(
            parse(r#"{"preset":"bw"}"#).to_vtracer().clustering,
            Clustering::Binary
        );
    }

    #[test]
    fn dials_override_and_are_clamped() {
        let config = parse(
            r#"{"preset":"poster","colorPrecision":42,"layerDifference":-3,
                "filterSpeckle":100000,"cornerThreshold":400,"mode":"polygon"}"#,
        )
        .to_vtracer();
        assert_eq!(config.color_precision, 8);
        assert_eq!(config.layer_difference, 0);
        assert_eq!(config.filter_speckle, 128);
        assert_eq!(config.corner_threshold, 180);
        assert_eq!(config.mode, FitMode::Polygon);
    }

    #[test]
    fn an_unknown_field_is_refused_rather_than_ignored() {
        assert!(serde_json::from_str::<TraceConfig>(r#"{"preset":"bw","speckle":3}"#).is_err());
        assert!(serde_json::from_str::<TraceConfig>(r#"{"preset":"sketch"}"#).is_err());
    }
}
