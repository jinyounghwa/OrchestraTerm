use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutNode {
    Leaf(usize),
    Split {
        axis: SplitAxis,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: usize,
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCore {
    pub name: String,
    pub panes: Vec<Pane>,
    pub layout: LayoutNode,
    pub focused_pane: usize,
    pub zoomed: bool,
    next_id: usize,
}

impl SessionCore {
    pub fn new(name: impl Into<String>) -> Self {
        let pane = Pane {
            id: 0,
            title: "Pane 0".to_string(),
            lines: vec!["OrchestraTerm ready".to_string()],
        };
        Self {
            name: name.into(),
            panes: vec![pane],
            layout: LayoutNode::Leaf(0),
            focused_pane: 0,
            zoomed: false,
            next_id: 1,
        }
    }

    pub fn pane_ids(&self) -> Vec<usize> {
        self.panes.iter().map(|p| p.id).collect()
    }

    pub fn split_focused(&mut self, axis: SplitAxis) {
        let new_id = self.next_id;
        self.next_id += 1;

        self.panes.push(Pane {
            id: new_id,
            title: format!("Pane {new_id}"),
            lines: vec![format!("split from pane {}", self.focused_pane)],
        });

        self.layout = Self::split_leaf(self.layout.clone(), self.focused_pane, new_id, axis);
        self.focused_pane = new_id;
    }

    pub fn close_focused(&mut self) {
        if self.panes.len() == 1 {
            return;
        }

        let removed = self.focused_pane;
        self.panes.retain(|p| p.id != removed);
        self.layout = Self::remove_leaf(self.layout.clone(), removed)
            .unwrap_or_else(|| LayoutNode::Leaf(self.panes[0].id));
        self.focused_pane = self.panes[0].id;
        self.zoomed = false;
    }

    pub fn focus_next(&mut self) {
        let ids = self.pane_ids();
        if ids.is_empty() {
            return;
        }
        if let Some(pos) = ids.iter().position(|id| *id == self.focused_pane) {
            self.focused_pane = ids[(pos + 1) % ids.len()];
        }
    }

    pub fn focus_prev(&mut self) {
        let ids = self.pane_ids();
        if ids.is_empty() {
            return;
        }
        if let Some(pos) = ids.iter().position(|id| *id == self.focused_pane) {
            let next = if pos == 0 { ids.len() - 1 } else { pos - 1 };
            self.focused_pane = ids[next];
        }
    }

    pub fn toggle_zoom(&mut self) {
        self.zoomed = !self.zoomed;
    }

    pub fn append_line_focused(&mut self, line: impl Into<String>) {
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == self.focused_pane) {
            pane.lines.push(line.into());
            while pane.lines.len() > 200 {
                pane.lines.remove(0);
            }
        }
    }

    pub fn append_line_to_pane(&mut self, pane_id: usize, line: impl Into<String>) {
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == pane_id) {
            pane.lines.push(line.into());
            while pane.lines.len() > 400 {
                pane.lines.remove(0);
            }
        }
    }

    fn split_leaf(node: LayoutNode, target: usize, new_id: usize, axis: SplitAxis) -> LayoutNode {
        match node {
            LayoutNode::Leaf(id) if id == target => LayoutNode::Split {
                axis,
                first: Box::new(LayoutNode::Leaf(id)),
                second: Box::new(LayoutNode::Leaf(new_id)),
            },
            LayoutNode::Leaf(_) => node,
            LayoutNode::Split {
                axis: current,
                first,
                second,
            } => LayoutNode::Split {
                axis: current,
                first: Box::new(Self::split_leaf(*first, target, new_id, axis)),
                second: Box::new(Self::split_leaf(*second, target, new_id, axis)),
            },
        }
    }

    fn remove_leaf(node: LayoutNode, target: usize) -> Option<LayoutNode> {
        match node {
            LayoutNode::Leaf(id) if id == target => None,
            LayoutNode::Leaf(id) => Some(LayoutNode::Leaf(id)),
            LayoutNode::Split {
                axis,
                first,
                second,
            } => {
                let left = Self::remove_leaf(*first, target);
                let right = Self::remove_leaf(*second, target);
                match (left, right) {
                    (Some(l), Some(r)) => Some(LayoutNode::Split {
                        axis,
                        first: Box::new(l),
                        second: Box::new(r),
                    }),
                    (Some(l), None) => Some(l),
                    (None, Some(r)) => Some(r),
                    (None, None) => None,
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Shortcut {
    pub key: &'static str,
    pub action: &'static str,
}

pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        key: "Ctrl+B, S",
        action: "Split horizontally",
    },
    Shortcut {
        key: "Ctrl+B, V",
        action: "Split vertically",
    },
    Shortcut {
        key: "Ctrl+B, X",
        action: "Close focused pane",
    },
    Shortcut {
        key: "Ctrl+B, Z",
        action: "Toggle zoom",
    },
    Shortcut {
        key: "Ctrl+B, ←/↑",
        action: "Focus previous pane",
    },
    Shortcut {
        key: "Ctrl+B, →/↓",
        action: "Focus next pane",
    },
    Shortcut {
        key: "Ctrl+Enter",
        action: "Send Enter to focused terminal",
    },
    Shortcut {
        key: "Cmd+O",
        action: "Select workspace folder",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_close_keeps_layout_valid() {
        let mut core = SessionCore::new("test");
        core.split_focused(SplitAxis::Vertical);
        core.split_focused(SplitAxis::Horizontal);
        assert_eq!(core.panes.len(), 3);
        core.close_focused();
        assert_eq!(core.panes.len(), 2);
    }

    #[test]
    fn focus_cycle_works() {
        let mut core = SessionCore::new("test");
        core.split_focused(SplitAxis::Vertical);
        let first = core.focused_pane;
        core.focus_prev();
        assert_ne!(core.focused_pane, first);
        core.focus_next();
        assert_eq!(core.focused_pane, first);
    }
}
