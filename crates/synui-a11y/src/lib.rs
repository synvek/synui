//! AccessKit accessibility tree builder.

use accesskit::{
    Action, Invalid, Node, NodeId, Role, TextPosition, TextSelection, Toggled, Tree, TreeUpdate,
};
use std::collections::HashMap;
use synui_core::{A11yAction, A11yProps, A11yRole, WidgetId, WidgetStore};

/// Maps SynUI widgets to AccessKit nodes.
pub struct A11yBridge {
    widget_to_node: HashMap<WidgetId, NodeId>,
    node_to_widget: HashMap<NodeId, WidgetId>,
    /// Text-run nodes for TextInput selection (AccessKit requires Role::TextRun).
    text_run_nodes: HashMap<WidgetId, NodeId>,
    next_id: u64,
    /// Bump when callers should push an update.
    pub generation: u64,
}

impl Default for A11yBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl A11yBridge {
    pub fn new() -> Self {
        Self {
            widget_to_node: HashMap::new(),
            node_to_widget: HashMap::new(),
            text_run_nodes: HashMap::new(),
            next_id: 1,
            generation: 0,
        }
    }

    pub fn mark_dirty(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn node_for_widget(&mut self, widget: WidgetId) -> NodeId {
        if let Some(&id) = self.widget_to_node.get(&widget) {
            return id;
        }
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.widget_to_node.insert(widget, id);
        self.node_to_widget.insert(id, widget);
        id
    }

    fn text_run_for_widget(&mut self, widget: WidgetId) -> NodeId {
        if let Some(&id) = self.text_run_nodes.get(&widget) {
            return id;
        }
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.text_run_nodes.insert(widget, id);
        id
    }

    pub fn widget_for_node(&self, node_id: NodeId) -> Option<WidgetId> {
        self.node_to_widget.get(&node_id).copied()
    }

    pub fn build_tree_update(
        &mut self,
        store: &WidgetStore,
        focus: Option<WidgetId>,
    ) -> TreeUpdate {
        // Widget IDs are never reused, so pruning removed widgets before each
        // rebuild prevents stale AccessKit reverse routes and unbounded maps.
        self.widget_to_node
            .retain(|widget, _| store.get(*widget).is_some());
        self.node_to_widget
            .retain(|_, widget| store.get(*widget).is_some());
        self.text_run_nodes
            .retain(|widget, _| store.get(*widget).is_some());

        let root_widget = store.root().unwrap();
        let root_node = self.node_for_widget(root_widget);
        let mut nodes = Vec::new();
        self.build_nodes(store, root_widget, root_node, &mut nodes);
        let focus_node = focus
            .and_then(|w| self.widget_to_node.get(&w).copied())
            .unwrap_or(root_node);
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(root_node)),
            tree_id: accesskit::TreeId::ROOT,
            focus: focus_node,
        }
    }

    fn build_nodes(
        &mut self,
        store: &WidgetStore,
        widget_id: WidgetId,
        node_id: NodeId,
        nodes: &mut Vec<(NodeId, Node)>,
    ) {
        let props = store.effective_a11y(widget_id).unwrap_or_default();
        let mut node = Node::new(map_role(props.role));
        apply_props(&mut node, &props);
        if let Some(sel) = &props.text_selection {
            let run_id = self.text_run_for_widget(widget_id);
            let value = props.value.clone().unwrap_or_default();
            let char_len = value.chars().count();
            let mut run = Node::new(Role::TextRun);
            // One unit per Unicode scalar — good enough for ASCII / simple scripts.
            let lengths: Vec<u8> = value.chars().map(|c| c.len_utf8() as u8).collect();
            if !lengths.is_empty() {
                run.set_character_lengths(lengths);
            }
            let anchor_i = byte_to_char_index(&value, sel.start).min(char_len);
            let focus_i = byte_to_char_index(&value, sel.end).min(char_len);
            node.set_text_selection(TextSelection {
                anchor: TextPosition {
                    node: run_id,
                    character_index: anchor_i,
                },
                focus: TextPosition {
                    node: run_id,
                    character_index: focus_i,
                },
            });
            node.set_children(vec![run_id]);
            node.add_action(Action::SetTextSelection);
            nodes.push((run_id, run));
        }
        if props.text_selection.is_none() {
            let child_widgets: Vec<WidgetId> = store.children_of(widget_id).to_vec();
            let child_nodes: Vec<NodeId> = child_widgets
                .iter()
                .map(|&w| {
                    let nid = self.node_for_widget(w);
                    self.build_nodes(store, w, nid, nodes);
                    nid
                })
                .collect();
            if !child_nodes.is_empty() {
                node.set_children(child_nodes);
            }
        }
        nodes.push((node_id, node));
    }
}

fn apply_props(node: &mut Node, props: &A11yProps) {
    if props.disabled {
        node.set_disabled();
    }
    if let Some(checked) = props.checked {
        node.set_toggled(if checked {
            Toggled::True
        } else {
            Toggled::False
        });
    }
    if let Some(selected) = props.selected {
        node.set_selected(selected);
    }
    if let Some(expanded) = props.expanded {
        node.set_expanded(expanded);
    }
    if props.read_only {
        node.set_read_only();
    }
    if props.required {
        node.set_required();
    }
    if props.invalid {
        node.set_invalid(Invalid::True);
    }
    if let Some(label) = &props.label {
        node.set_label(label.clone());
    }
    if let Some(value) = &props.value {
        node.set_value(value.clone());
    }
    // Logical dimensions for virtual List/Grid stay metadata-only; build_nodes
    // never manufactures one accessibility node per logical row.
    if let Some(rows) = props.row_count {
        node.set_row_count(rows);
    }
    if let Some(cols) = props.col_count {
        node.set_column_count(cols);
    }
    if let Some(row) = props.row_index {
        node.set_row_index(row);
    }
    if let Some(column) = props.col_index {
        node.set_column_index(column);
    }
    for action in &props.actions {
        node.add_action(match action {
            A11yAction::Click => Action::Click,
            A11yAction::Focus => Action::Focus,
            A11yAction::SetValue => Action::SetValue,
        });
    }
}

fn byte_to_char_index(s: &str, byte: usize) -> usize {
    let mut byte = byte.min(s.len());
    // AccessKit / widgets may pass mid-codepoint offsets; snap to a char boundary.
    while byte > 0 && !s.is_char_boundary(byte) {
        byte -= 1;
    }
    s[..byte].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_to_char_snaps_mid_em_dash() {
        let s = "Heading\nParagraph — select";
        // '—' spans bytes 18..21 in this string.
        assert!(s.is_char_boundary(18));
        assert!(!s.is_char_boundary(20));
        let at_dash = byte_to_char_index(s, 18);
        let mid = byte_to_char_index(s, 20);
        assert_eq!(mid, at_dash);
    }

    #[test]
    fn semantic_props_map_state_actions_and_virtual_grid_metadata() {
        let props = A11yProps {
            role: A11yRole::Grid,
            label: Some("Orders".into()),
            value: Some("ready".into()),
            actions: vec![A11yAction::Click, A11yAction::Focus, A11yAction::SetValue],
            disabled: true,
            checked: Some(true),
            selected: Some(true),
            expanded: Some(false),
            read_only: true,
            required: true,
            invalid: true,
            row_count: Some(100_000),
            col_count: Some(24),
            row_index: Some(72),
            col_index: Some(5),
            ..Default::default()
        };
        let mut node = Node::new(map_role(props.role));
        apply_props(&mut node, &props);

        assert_eq!(node.role(), Role::Grid);
        assert_eq!(node.label(), Some("Orders"));
        assert_eq!(node.value(), Some("ready"));
        assert!(node.is_disabled());
        assert_eq!(node.toggled(), Some(Toggled::True));
        assert_eq!(node.is_selected(), Some(true));
        assert_eq!(node.is_expanded(), Some(false));
        assert!(node.is_read_only());
        assert!(node.is_required());
        assert_eq!(node.invalid(), Some(Invalid::True));
        assert_eq!(node.row_count(), Some(100_000));
        assert_eq!(node.column_count(), Some(24));
        assert_eq!(node.row_index(), Some(72));
        assert_eq!(node.column_index(), Some(5));
        assert!(node.supports_action(Action::Click));
        assert!(node.supports_action(Action::Focus));
        assert!(node.supports_action(Action::SetValue));
    }
}

fn map_role(role: A11yRole) -> Role {
    match role {
        A11yRole::Window => Role::Window,
        A11yRole::Button => Role::Button,
        A11yRole::Label => Role::Label,
        A11yRole::TextInput => Role::TextInput,
        A11yRole::Checkbox => Role::CheckBox,
        A11yRole::Switch => Role::Switch,
        A11yRole::Slider => Role::Slider,
        A11yRole::ProgressBar => Role::ProgressIndicator,
        A11yRole::ListItem => Role::ListItem,
        A11yRole::Tab => Role::Tab,
        A11yRole::Dialog => Role::Dialog,
        A11yRole::Status => Role::Status,
        A11yRole::Generic => Role::GenericContainer,
        A11yRole::List => Role::List,
        A11yRole::Grid => Role::Grid,
        A11yRole::Row => Role::Row,
        A11yRole::GridCell => Role::Cell,
    }
}
