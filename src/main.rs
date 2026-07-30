mod herdr;
mod socket;

use herdr::Context;
use socket::Client;

/// Direction-only nav: same-pane move if not at the edge, otherwise
/// cross into the adjacent tab and land on its entry-side pane.
fn nav(client: &mut Client, direction: &str) -> Result<(), String> {
    let edges = herdr::pane_edges(client, None)?;
    if herdr::edge_flag(&edges, direction)? {
        let ctx = Context::resolve(client)?;
        herdr::cycle_tab(client, &ctx.workspace_id, &ctx.tab_id, direction)
    } else {
        herdr::pane_focus_direction(client, None, direction)
    }
}

/// Tab-first nav: step within the current workspace's tabs, only
/// crossing into the adjacent workspace once already at the tab edge.
fn spatial_tab(client: &mut Client, direction: &str) -> Result<(), String> {
    let ctx = Context::resolve(client)?;
    herdr::spatial_cycle_tab(client, &ctx.workspace_id, &ctx.tab_id, direction)
}

fn run(mode: &str, direction: &str) -> Result<(), String> {
    let mut client = Client::new(herdr::socket_path());

    match mode {
        "nav" => nav(&mut client, direction),
        "spatial-tab" => spatial_tab(&mut client, direction),
        "cycle-workspace" => herdr::cycle_workspace(&mut client, direction),
        other => Err(format!("unknown mode '{other}'")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mode, direction) = match args.as_slice() {
        [_, mode, direction] if direction == "left" || direction == "right" => {
            (mode.as_str(), direction.as_str())
        }
        _ => {
            eprintln!("usage: herdr-nav <nav|spatial-tab|cycle-workspace> <left|right>");
            std::process::exit(2);
        }
    };

    if let Err(message) = run(mode, direction) {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
