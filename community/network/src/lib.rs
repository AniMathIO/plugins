//! Net Lab — deterministic, Packet Tracer-like networking animations as an
//! AniMathIO clip-source plugin.
//!
//! The render is a pure function of the parameters and `progress`: a small toy
//! topology plays a protocol scenario event by event (each event = one packet
//! crossing one link hop), while MAC/ARP/route tables fill in as a replay of the
//! events completed so far. Protocol sequencing studied in the MIT netsim
//! projects; all Rust code and all artwork are original — see NOTICE.

wit_bindgen::generate!({ path: "wit", world: "source-plugin" });
use animathio::plugin::types::*;

mod draw;
mod params;
mod scenario;
mod tables;
#[cfg(test)]
mod tests;
mod timeline;
mod topology;

use draw::{DrawInput, Labels};
use params::{
    bool_param, boolean, color_of, color_param, enum_param, error, num_param, number, one_of,
};
use scenario::{Scenario, TableKind, SCENARIOS, TABLES};

const ACCENT_DEFAULT: Color = Color {
    r: 79,
    g: 195,
    b: 247,
    a: 255,
};
const TOPOLOGIES: [&str; 2] = ["single-switch", "through-router"];

struct NetLab;

impl Guest for NetLab {
    fn describe() -> Manifest {
        Manifest {
            name: "Net Lab".into(),
            version: "1.0.0".into(),
            description:
                "Animate ARP, ICMP ping, TCP, DHCP, DNS and traceroute over teaching topologies"
                    .into(),
            kinds: vec!["clip-source".into()],
            metadata: vec![
                ("param.progress.unit".into(), "0..1 timeline".into()),
                ("param.packet_size.unit".into(), "% of short edge".into()),
            ],
            params: vec![
                enum_param("scenario", "Scenario", &SCENARIOS, "ping"),
                enum_param("topology", "Topology", &TOPOLOGIES, "through-router"),
                num_param("progress", "Progress", 0.0, 1.0, 1.0, 0.01),
                num_param("steps", "Steps (max events)", 1.0, 200.0, 24.0, 1.0),
                enum_param("labels", "Labels", &["full", "brief", "none"], "full"),
                bool_param("show_tables", "Show table panel", true),
                enum_param("table", "Table", &TABLES, "auto"),
                num_param("packet_size", "Packet size (%)", 0.5, 6.0, 2.4, 0.1),
                color_param("accent", "Accent", ACCENT_DEFAULT),
                bool_param("show_step_list", "Show step list", true),
                bool_param("speed_lines", "Speed lines", true),
            ],
        }
    }

    fn render_clip(
        params: Vec<ParamEntry>,
        _time_ticks: i64,
        size: Size,
    ) -> Result<Scene, Diagnostic> {
        render(&params, size)
    }
}

fn render(params: &[ParamEntry], size: Size) -> Result<Scene, Diagnostic> {
    if size.width == 0 || size.height == 0 {
        return Err(error("Canvas dimensions must be positive"));
    }
    let scenario = Scenario::parse(one_of(params, "scenario", "ping", &SCENARIOS)?)
        .ok_or_else(|| error("Unsupported scenario"))?;
    let through_router =
        one_of(params, "topology", "through-router", &TOPOLOGIES)? == "through-router";
    let topo = topology::topology(through_router, scenario == Scenario::Dns);
    let labels = match one_of(params, "labels", "full", &["full", "brief", "none"])? {
        "brief" => Labels::Brief,
        "none" => Labels::None,
        _ => Labels::Full,
    };
    let table_kind = match one_of(params, "table", "auto", &TABLES)? {
        "mac" => TableKind::Mac,
        "arp" => TableKind::Arp,
        "route" => TableKind::Route,
        _ => TableKind::Auto,
    };
    let progress = number(params, "progress", 1.0, 0.0, 1.0)?;
    let steps = number(params, "steps", 24.0, 1.0, 200.0)? as usize;
    let packet_percent = number(params, "packet_size", 2.4, 0.5, 6.0)?;
    let show_tables = boolean(params, "show_tables", true);
    let show_step_list = boolean(params, "show_step_list", true);
    let speed_lines = boolean(params, "speed_lines", true);
    let accent = color_of(params, "accent", ACCENT_DEFAULT);

    let mut events = scenario::build(scenario, topo);
    events.truncate(steps);
    let u = timeline::position(progress, events.len());
    Ok(draw::draw(
        &DrawInput {
            topo,
            events: &events,
            scenario,
            u,
            labels,
            table_kind,
            show_tables,
            show_step_list,
            speed_lines,
            packet_percent,
            accent,
        },
        &size,
    ))
}

export!(NetLab);
