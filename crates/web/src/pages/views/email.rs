//! The protocol's views: none yet.

use netray_model::evidence::Block;

use super::ViewContext;

pub(super) fn render(_block: &Block, _ctx: &ViewContext<'_>) -> Option<String> {
    None
}
