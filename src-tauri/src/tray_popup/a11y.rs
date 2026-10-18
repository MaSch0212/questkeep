//! The tray popup's accessibility tree (AccessKit), built from the same [`Layout`] the renderer
//! draws, so screen readers see exactly what is on screen: the sections as groups and every item as
//! a button named after its game, store or action, with the keyboard focus. Platform independent;
//! the Windows adapter in `window.rs` exposes it through UI Automation.

use accesskit::{
    Action as A11yAction, Affine, Node, NodeId, Rect as A11yRect, Role, TreeId, TreeInfo,
    TreeUpdate,
};

use super::PopupData;
use super::layout::{self, ItemKind, Layout, Rect};

const ROOT: NodeId = NodeId(1);
const RECENT_GROUP: NodeId = NodeId(2);
const FAVORITES_GROUP: NodeId = NodeId(3);
const STORES_GROUP: NodeId = NodeId(4);
/// Items are `ITEM_BASE + index in the layout`.
const ITEM_BASE: u64 = 1000;

pub fn item_node(index: usize) -> NodeId {
    NodeId(ITEM_BASE + index as u64)
}

/// The layout index of an item node.
pub fn item_index(node: NodeId) -> Option<usize> {
    node.0
        .checked_sub(ITEM_BASE)
        .and_then(|i| usize::try_from(i).ok())
}

fn bounds(r: Rect) -> A11yRect {
    A11yRect::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.right()),
        f64::from(r.bottom()),
    )
}

/// What the screen reader calls an item.
fn item_label(data: &PopupData, kind: ItemKind) -> String {
    match kind {
        ItemKind::Tile(i) => data
            .games
            .recent
            .get(i)
            .map_or_else(String::new, |g| g.name.clone()),
        ItemKind::Row(i) => data
            .games
            .favorites
            .get(i)
            .map_or_else(String::new, |g| g.name.clone()),
        ItemKind::Pill(i) => data
            .clients
            .get(i)
            .map_or_else(String::new, |c| format!("Open {}", c.name)),
        ItemKind::OpenButton => layout::OPEN_LABEL.to_string(),
        ItemKind::ExitButton => format!("{} Questkeep", layout::EXIT_LABEL),
    }
}

fn group(label: &str, area: Option<Rect>, children: Vec<NodeId>) -> Node {
    let mut node = Node::new(Role::Group);
    node.set_label(label);
    node.set_children(children);
    if let Some(area) = area {
        node.set_bounds(bounds(area));
    }
    node
}

/// Smallest rectangle around the given items.
fn union(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    rects.reduce(|a, b| {
        let (x, y) = (a.x.min(b.x), a.y.min(b.y));
        Rect::new(
            x,
            y,
            a.right().max(b.right()) - x,
            a.bottom().max(b.bottom()) - y,
        )
    })
}

/// The full tree. `focus` is the focused item (the window itself when `None`); `scale` maps the
/// logical layout to physical window pixels.
pub fn tree(layout: &Layout, data: &PopupData, focus: Option<usize>, scale: f32) -> TreeUpdate {
    let mut nodes = Vec::with_capacity(layout.items.len() + 4);
    let (mut recent, mut favorites, mut stores, mut footer) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (index, item) in layout.items.iter().enumerate() {
        let id = item_node(index);
        let mut node = Node::new(Role::Button);
        node.set_label(item_label(data, item.kind));
        node.set_bounds(bounds(item.rect));
        node.add_action(A11yAction::Click);
        node.add_action(A11yAction::Focus);
        match item.kind {
            ItemKind::Tile(_) => {
                node.set_description("Play");
                recent.push(id);
            }
            ItemKind::Row(_) => {
                node.set_description("Play");
                node.add_action(A11yAction::ScrollIntoView);
                favorites.push(id);
            }
            ItemKind::Pill(_) => stores.push(id),
            ItemKind::OpenButton | ItemKind::ExitButton => footer.push(id),
        }
        nodes.push((id, node));
    }

    let rects = |ids: &[NodeId]| {
        union(
            ids.iter()
                .filter_map(|id| item_index(*id))
                .filter_map(|i| layout.items.get(i))
                .map(|item| item.rect),
        )
    };
    let mut root_children = Vec::new();
    if !recent.is_empty() || layout.empty_recent.is_some() {
        let mut label = layout::RECENT_LABEL.to_string();
        if recent.is_empty() {
            label = format!("{label}: {}", layout::EMPTY_RECENT);
        }
        let area = rects(&recent).or(layout.empty_recent);
        nodes.push((RECENT_GROUP, group(&label, area, recent)));
        root_children.push(RECENT_GROUP);
    }
    if !favorites.is_empty() {
        let mut node = group(layout::FAVORITES_LABEL, layout.favorites_clip, favorites);
        node.set_clips_children();
        nodes.push((FAVORITES_GROUP, node));
        root_children.push(FAVORITES_GROUP);
    }
    if !stores.is_empty() {
        let area = rects(&stores);
        nodes.push((STORES_GROUP, group(layout::STORES_LABEL, area, stores)));
        root_children.push(STORES_GROUP);
    }
    root_children.extend(footer);

    let mut root = Node::new(Role::Window);
    root.set_label("Questkeep");
    root.set_children(root_children);
    root.set_bounds(A11yRect::new(
        0.0,
        0.0,
        f64::from(layout.width),
        f64::from(layout.height),
    ));
    root.set_transform(Affine::scale(f64::from(scale)));
    nodes.push((ROOT, root));

    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(ROOT)),
        tree_id: TreeId::ROOT,
        focus: focus
            .filter(|&i| i < layout.items.len())
            .map_or(ROOT, item_node),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray_popup::layout::layout;
    use questkeep_core::platform::StoreClient;
    use questkeep_core::tray::{TrayGame, TrayGames};
    use uuid::Uuid;

    fn game(n: u128, name: &str) -> TrayGame {
        TrayGame {
            id: Uuid::from_u128(n),
            name: name.into(),
            icon: None,
            cover: None,
        }
    }

    fn data() -> PopupData {
        PopupData {
            games: TrayGames {
                recent: vec![game(1, "Hades"), game(2, "Celeste")],
                favorites: vec![game(3, "Hollow Knight")],
            },
            clients: vec![StoreClient {
                id: "steam",
                name: "Steam",
                uri: "steam://open/main",
            }],
        }
    }

    fn node(update: &TreeUpdate, id: NodeId) -> &Node {
        &update.nodes.iter().find(|(n, _)| *n == id).unwrap().1
    }

    #[test]
    fn every_item_is_a_named_button_in_its_section() {
        let d = data();
        let l = layout(&d.content(), 0.0, |t| t.len() as f32 * 7.0);
        let t = tree(&l, &d, None, 1.5);
        assert_eq!(t.focus, ROOT);
        assert_eq!(
            t.nodes.len(),
            l.items.len() + 4,
            "items, three groups, root"
        );

        let labels: Vec<_> = (0..l.items.len())
            .map(|i| {
                node(&t, item_node(i))
                    .label()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Hades",
                "Celeste",
                "Hollow Knight",
                "Open Steam",
                "Open Questkeep",
                "Exit Questkeep"
            ]
        );
        for i in 0..l.items.len() {
            let n = node(&t, item_node(i));
            assert_eq!(n.role(), Role::Button);
            assert!(n.supports_action(A11yAction::Click));
            assert!(n.bounds().is_some());
        }
        assert_eq!(
            node(&t, RECENT_GROUP).children(),
            &[item_node(0), item_node(1)]
        );
        assert_eq!(node(&t, FAVORITES_GROUP).children(), &[item_node(2)]);
        assert!(node(&t, FAVORITES_GROUP).clips_children());
        assert_eq!(
            node(&t, ROOT).children(),
            &[
                RECENT_GROUP,
                FAVORITES_GROUP,
                STORES_GROUP,
                item_node(4),
                item_node(5)
            ]
        );
        assert_eq!(node(&t, ROOT).transform(), Some(&Affine::scale(1.5)));
    }

    #[test]
    fn focus_follows_the_keyboard_focus() {
        let d = data();
        let l = layout(&d.content(), 0.0, |t| t.len() as f32 * 7.0);
        assert_eq!(tree(&l, &d, Some(2), 1.0).focus, item_node(2));
        assert_eq!(tree(&l, &d, Some(99), 1.0).focus, ROOT);
    }

    #[test]
    fn empty_popup_explains_itself() {
        let d = PopupData::default();
        let l = layout(&d.content(), 0.0, |t| t.len() as f32 * 7.0);
        let t = tree(&l, &d, None, 1.0);
        let label = node(&t, RECENT_GROUP)
            .label()
            .unwrap_or_default()
            .to_string();
        assert!(label.contains(layout::EMPTY_RECENT));
        assert!(node(&t, RECENT_GROUP).children().is_empty());
    }

    #[test]
    fn node_ids_map_back_to_items() {
        assert_eq!(item_index(item_node(7)), Some(7));
        assert_eq!(item_index(ROOT), None);
    }
}
