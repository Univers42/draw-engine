use std::collections::HashSet;

use crate::scene::element::DrawElement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZOrderMode {
    Front,
    Back,
    Forward,
    Backward,
}

impl ZOrderMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "front" => Some(Self::Front),
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "backward" => Some(Self::Backward),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Backward => "backward",
        }
    }
}

pub fn reorder_elements(
    live: &[DrawElement],
    ids: &HashSet<String>,
    mode: ZOrderMode,
) -> Vec<DrawElement> {
    let selected: Vec<DrawElement> = live
        .iter()
        .filter(|el| ids.contains(&el.id))
        .cloned()
        .collect();
    if selected.is_empty() {
        return live.to_vec();
    }
    let rest: Vec<DrawElement> = live
        .iter()
        .filter(|el| !ids.contains(&el.id))
        .cloned()
        .collect();
    match mode {
        ZOrderMode::Front => {
            let mut out = rest;
            out.extend(selected);
            out
        }
        ZOrderMode::Back => {
            let mut out = selected;
            out.extend(rest);
            out
        }
        ZOrderMode::Forward => {
            let mut out = live.to_vec();
            for i in (0..out.len().saturating_sub(1)).rev() {
                if ids.contains(&out[i].id) && !ids.contains(&out[i + 1].id) {
                    out.swap(i, i + 1);
                }
            }
            out
        }
        ZOrderMode::Backward => {
            let mut out = live.to_vec();
            for i in 1..out.len() {
                if ids.contains(&out[i].id) && !ids.contains(&out[i - 1].id) {
                    out.swap(i, i - 1);
                }
            }
            out
        }
    }
}
