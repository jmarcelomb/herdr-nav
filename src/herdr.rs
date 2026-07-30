use crate::socket::Client;
use serde_json::{Value, json};

/// Resolved identity of the pane this invocation should act on, plus
/// its parent workspace so tab/workspace cycling doesn't need an extra
/// `pane.current` round-trip in the common case.
pub struct Context {
    pub workspace_id: String,
    pub tab_id: String,
}

impl Context {
    /// Fast path: herdr injects `HERDR_PANE_ID`, `HERDR_TAB_ID`, and
    /// `HERDR_WORKSPACE_ID` into `plugin_action` invocations, so the
    /// common case needs zero socket round-trips to know where we are.
    /// `HERDR_PANE_ID` is checked only to confirm we are in that
    /// context at all; the pane id itself is never needed downstream.
    pub fn from_env() -> Option<Self> {
        std::env::var("HERDR_PANE_ID").ok()?;
        Some(Self {
            workspace_id: std::env::var("HERDR_WORKSPACE_ID").ok()?,
            tab_id: std::env::var("HERDR_TAB_ID").ok()?,
        })
    }

    /// Fallback for running outside a herdr plugin_action invocation
    /// (e.g. manual testing from a shell): ask the server which pane
    /// is currently focused.
    pub fn from_active_pane(client: &mut Client) -> Result<Self, String> {
        let result = client.call("pane.current", json!({}))?;
        let pane = result
            .get("pane")
            .ok_or("pane.current: missing pane field")?;
        let field = |key: &str| -> Result<String, String> {
            pane.get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("pane.current: missing {key}"))
        };
        Ok(Self {
            workspace_id: field("workspace_id")?,
            tab_id: field("tab_id")?,
        })
    }

    pub fn resolve(client: &mut Client) -> Result<Self, String> {
        match Self::from_env() {
            Some(ctx) => Ok(ctx),
            None => Self::from_active_pane(client),
        }
    }
}

pub fn socket_path() -> String {
    std::env::var("HERDR_SOCKET_PATH").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{home}/.config/herdr/herdr.sock")
    })
}

/// `pane.edges`, optionally scoped to `pane_id` (omit to target the
/// server's active focused pane).
pub fn pane_edges(client: &mut Client, pane_id: Option<&str>) -> Result<Value, String> {
    let result = client.call("pane.edges", json!({ "pane_id": pane_id }))?;
    result
        .get("edges")
        .cloned()
        .ok_or_else(|| "pane.edges: missing edges field".to_string())
}

pub fn edge_flag(edges: &Value, direction: &str) -> Result<bool, String> {
    edges
        .get(direction)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("pane.edges: missing '{direction}' flag"))
}

/// `pane.focus_direction`, optionally scoped to `pane_id` (omit to
/// target the server's active focused pane).
pub fn pane_focus_direction(
    client: &mut Client,
    pane_id: Option<&str>,
    direction: &str,
) -> Result<(), String> {
    client.call(
        "pane.focus_direction",
        json!({ "pane_id": pane_id, "direction": direction }),
    )?;
    Ok(())
}

/// Step the active focused pane toward `direction` until it hits that
/// edge, so a tab/workspace switch lands the same-side pane the old
/// context had. Guarded like the original Python scripts to avoid an
/// infinite loop against an unexpected layout.
pub fn focus_edge_pane(client: &mut Client, direction: &str) -> Result<(), String> {
    for _ in 0..64 {
        let edges = pane_edges(client, None)?;
        if edge_flag(&edges, direction)? {
            return Ok(());
        }
        pane_focus_direction(client, None, direction)?;
    }
    Ok(())
}

pub fn opposite(direction: &str) -> &'static str {
    if direction == "left" { "right" } else { "left" }
}

struct Ordered {
    id: String,
    number: u64,
    focused: bool,
}

fn parse_ordered(items: &[Value], id_key: &str) -> Vec<Ordered> {
    let mut out: Vec<Ordered> = items
        .iter()
        .filter_map(|item| {
            Some(Ordered {
                id: item.get(id_key)?.as_str()?.to_string(),
                number: item.get("number")?.as_u64()?,
                focused: item.get("focused")?.as_bool()?,
            })
        })
        .collect();
    out.sort_by_key(|entry| entry.number);
    out
}

/// Pick the id adjacent to `current_id` in `direction`, wrapping
/// around. Returns `None` when there is nothing to cycle to.
fn adjacent(items: &[Ordered], current_id: &str, direction: &str) -> Option<String> {
    if items.len() < 2 {
        return None;
    }
    let ids: Vec<&str> = items.iter().map(|entry| entry.id.as_str()).collect();
    let current_index = ids.iter().position(|id| *id == current_id)?;
    let len = ids.len();
    let target_index = if direction == "left" {
        (current_index + len - 1) % len
    } else {
        (current_index + 1) % len
    };
    Some(ids[target_index].to_string())
}

/// Pick the id adjacent to `current_id` in `direction` without
/// wrapping. Returns `None` when `current_id` is already the edge
/// item in that direction (or not found), signaling the caller should
/// cross a boundary instead of wrapping in place.
fn adjacent_no_wrap(items: &[Ordered], current_id: &str, direction: &str) -> Option<String> {
    let ids: Vec<&str> = items.iter().map(|entry| entry.id.as_str()).collect();
    let current_index = ids.iter().position(|id| *id == current_id)?;
    if direction == "left" {
        current_index.checked_sub(1).map(|i| ids[i].to_string())
    } else {
        ids.get(current_index + 1).map(|id| id.to_string())
    }
}

/// The tab to land on when entering a workspace from `direction`: the
/// first tab when arriving from the left (moving right), the last tab
/// when arriving from the right (moving left), mirroring how
/// `focus_edge_pane` aligns the pane within that tab.
fn entry_tab(tabs: &[Ordered], direction: &str) -> Option<String> {
    let tab = if direction == "right" {
        tabs.first()
    } else {
        tabs.last()
    };
    tab.map(|entry| entry.id.clone())
}

fn list_tabs(client: &mut Client, workspace_id: &str) -> Result<Vec<Ordered>, String> {
    let result = client.call("tab.list", json!({ "workspace_id": workspace_id }))?;
    let tabs = result
        .get("tabs")
        .and_then(Value::as_array)
        .ok_or("tab.list: missing tabs array")?;
    Ok(parse_ordered(tabs, "tab_id"))
}

fn list_workspaces(client: &mut Client) -> Result<Vec<Ordered>, String> {
    let result = client.call("workspace.list", json!({}))?;
    let workspaces = result
        .get("workspaces")
        .and_then(Value::as_array)
        .ok_or("workspace.list: missing workspaces array")?;
    Ok(parse_ordered(workspaces, "workspace_id"))
}

/// Move focus to the next/previous tab within `workspace_id`, then
/// re-align the focused pane to the entry edge so the move feels like
/// sliding across a continuous strip of panes.
pub fn cycle_tab(
    client: &mut Client,
    workspace_id: &str,
    current_tab_id: &str,
    direction: &str,
) -> Result<(), String> {
    let ordered = list_tabs(client, workspace_id)?;
    let Some(target_tab_id) = adjacent(&ordered, current_tab_id, direction) else {
        return Ok(());
    };

    client.call("tab.focus", json!({ "tab_id": target_tab_id }))?;
    focus_edge_pane(client, opposite(direction))
}

/// Move focus to the next/previous tab, treating tabs as a single
/// strip that spans workspace boundaries: step within the current
/// workspace's tabs first, and only cross into the adjacent workspace
/// (landing on its entry-side tab) once already at that tab edge.
/// Falls back to wrapping within the current workspace's tabs when
/// there is nowhere else to go (e.g. only one workspace exists).
pub fn spatial_cycle_tab(
    client: &mut Client,
    workspace_id: &str,
    current_tab_id: &str,
    direction: &str,
) -> Result<(), String> {
    let tabs = list_tabs(client, workspace_id)?;

    if let Some(target_tab_id) = adjacent_no_wrap(&tabs, current_tab_id, direction) {
        client.call("tab.focus", json!({ "tab_id": target_tab_id }))?;
        return focus_edge_pane(client, opposite(direction));
    }

    let workspaces = list_workspaces(client)?;
    if let Some(target_workspace_id) = adjacent_no_wrap(&workspaces, workspace_id, direction) {
        client.call(
            "workspace.focus",
            json!({ "workspace_id": target_workspace_id }),
        )?;
        let target_tabs = list_tabs(client, &target_workspace_id)?;
        if let Some(target_tab_id) = entry_tab(&target_tabs, direction) {
            client.call("tab.focus", json!({ "tab_id": target_tab_id }))?;
        }
        return focus_edge_pane(client, opposite(direction));
    }

    // Nowhere to cross into (single workspace): wrap within the
    // current workspace's tabs instead of doing nothing.
    if let Some(target_tab_id) = adjacent(&tabs, current_tab_id, direction) {
        client.call("tab.focus", json!({ "tab_id": target_tab_id }))?;
        return focus_edge_pane(client, opposite(direction));
    }

    Ok(())
}

/// Move focus to the next/previous workspace, then re-align the
/// focused pane to the entry edge.
pub fn cycle_workspace(client: &mut Client, direction: &str) -> Result<(), String> {
    let ordered = list_workspaces(client)?;
    let Some(current_id) = ordered
        .iter()
        .find(|entry| entry.focused)
        .map(|entry| entry.id.clone())
    else {
        return Ok(());
    };
    let Some(target_workspace_id) = adjacent(&ordered, &current_id, direction) else {
        return Ok(());
    };

    client.call(
        "workspace.focus",
        json!({ "workspace_id": target_workspace_id }),
    )?;
    focus_edge_pane(client, opposite(direction))
}
