use serde::{Deserialize, Serialize};

use crate::scene::element::DrawElement;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OsidrawFile {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u32,
    pub elements: Vec<DrawElement>,
}

pub fn scene_to_json(elements: &[DrawElement]) -> String {
    let file = OsidrawFile {
        kind: "osidraw".into(),
        version: 1,
        elements: elements
            .iter()
            .filter(|el| !el.is_deleted)
            .cloned()
            .collect(),
    };
    serde_json::to_string_pretty(&file)
        .unwrap_or_else(|_| "{\"type\":\"osidraw\",\"version\":1,\"elements\":[]}".into())
}

pub fn elements_from_json(json: &str) -> Option<Vec<DrawElement>> {
    let data: serde_json::Value = serde_json::from_str(json).ok()?;
    if data.get("type")?.as_str()? != "osidraw" {
        return None;
    }
    let mut elements: Vec<DrawElement> = data
        .get("elements")
        .and_then(|value| serde_json::from_value(value.clone()).ok())?;
    // Boards saved before groups could nest carry a single `groupId`. Folded here, at
    // the one door scenes come in through, so nothing downstream has to know the old
    // spelling ever existed.
    for element in &mut elements {
        crate::scene::element::normalize_group_ids(element);
    }
    Some(elements)
}
