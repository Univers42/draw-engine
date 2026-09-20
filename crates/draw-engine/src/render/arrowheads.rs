use crate::scene::element::{Arrowhead, DrawElement};

pub fn default_arrowhead(element: &DrawElement, end: &str) -> Arrowhead {
    let explicit = if end == "start" {
        element.start_arrowhead
    } else {
        element.end_arrowhead
    };
    if let Some(kind) = explicit {
        return kind;
    }
    if element.kind == crate::scene::DrawElementType::Arrow && end == "end" {
        Arrowhead::Arrow
    } else {
        Arrowhead::None
    }
}
