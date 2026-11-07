use craft_ui::docking::{Layout, Location, Node, Permissions, Placement};
use craft_ui::layout::{SplitAxis, SplitSize};

use super::Panel;

/// The saved panel arrangement
pub type Workspace = craft_ui::docking::Workspace<Panel>;

/// The graph stays docked and separate from other tabs
pub fn permissions(panel: &Panel) -> Permissions {
    if *panel == Panel::Graph { Permissions { accepts_splits: true, ..Permissions::PROTECTED } } else { Permissions::default() }
}

fn tabs(panels: &[Panel]) -> Node<Panel> {
    Node::Tabs { panels: panels.to_vec(), active: 0 }
}

fn split(axis: SplitAxis, ratio: f32, first: Node<Panel>, second: Node<Panel>) -> Node<Panel> {
    Node::Split { axis, size: SplitSize::Ratio(ratio), first: Box::new(first), second: Box::new(second) }
}

/// Default workspace with console and history initially hidden
pub fn default_workspace() -> Workspace {
    use Panel::*;
    let left = split(SplitAxis::Vertical, 0.36, tabs(&[Explorer]), tabs(&[Library]));
    let views = split(SplitAxis::Horizontal, 0.5, tabs(&[View3d]), tabs(&[View2d]));
    let center = split(SplitAxis::Vertical, 0.52, tabs(&[Graph]), views);
    let root = split(SplitAxis::Horizontal, 0.15, left, split(SplitAxis::Horizontal, 0.8, center, tabs(&[Properties])));
    let mut workspace = Workspace::new(Layout { root: Some(root), floating: Vec::new() });
    for panel in Panel::ALL {
        if let Some(rect) = panel.float_rect() {
            let location = Location {
                floating: Some(rect),
                anchor: None,
                placement: Placement::Tab { before: None },
                size: None,
                stack: None,
                sibling: Vec::new(),
                group: None,
            };
            workspace.hidden.push((panel, location));
        }
    }
    workspace
}

/// Validate the workspace and protect the central graph panel
pub fn validate(workspace: &Workspace) -> Result<(), String> {
    workspace.validate().map_err(|error| error.to_string())?;
    let layout = &workspace.layout;
    if layout.floating.iter().any(|window| window.contains(&Panel::Graph)) {
        return Err("The graph cannot float".into());
    }
    if workspace.hidden.iter().any(|(panel, _)| *panel == Panel::Graph) {
        return Err("The graph cannot be hidden".into());
    }
    let mut found = false;
    let mut nodes: Vec<&Node<Panel>> = layout.root.iter().collect();
    while let Some(node) = nodes.pop() {
        match node {
            Node::Split { first, second, .. } => {
                nodes.push(first);
                nodes.push(second);
            }
            Node::Tabs { panels, .. } if panels.contains(&Panel::Graph) => {
                if panels.len() != 1 {
                    return Err("The graph cannot share a tab group".into());
                }
                found = true;
            }
            Node::Stack { entries } if entries.iter().any(|entry| entry.panel == Panel::Graph) => {
                return Err("The graph cannot be collapsed".into());
            }
            Node::Dock { columns, .. } if columns.iter().flat_map(|column| &column.groups).any(|group| group.panels.contains(&Panel::Graph)) => {
                return Err("The graph cannot be in a dock".into());
            }
            _ => {}
        }
    }
    if found { Ok(()) } else { Err("The panel layout must contain the graph".into()) }
}
