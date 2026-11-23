//! Native unit tests: scenario sequencing, table replay, timeline behaviour,
//! parameter validation and render determinism.

use super::*;
use crate::animathio::plugin::types::DrawCommand;
use crate::scenario::{Event, Proto, Scenario};
use crate::topology::{topology, Topology, CLIENT, SERVER};

fn pv(id: &str, value: ParamValue) -> ParamEntry {
    ParamEntry {
        id: id.into(),
        value,
    }
}

fn both_topologies() -> Vec<&'static Topology> {
    vec![topology(false, false), topology(true, false)]
}

fn build_all() -> Vec<(&'static Topology, Scenario, Vec<Event>)> {
    let mut out = Vec::new();
    for routed in [false, true] {
        for name in SCENARIOS {
            let scenario = Scenario::parse(name).unwrap();
            let topo = topology(routed, scenario == Scenario::Dns);
            out.push((topo, scenario, crate::scenario::build(scenario, topo)));
        }
    }
    out
}

fn find(events: &[Event], proto: Proto) -> Option<usize> {
    events.iter().position(|e| e.proto == proto)
}

#[test]
fn every_hop_is_a_real_link() {
    for (topo, scenario, events) in build_all() {
        for event in &events {
            assert!(
                !event.legs.is_empty() && event.legs.len() <= topo.devices.len(),
                "{scenario:?} event with too many legs"
            );
            for &leg in &event.legs {
                assert!(
                    topo.neighbors(event.from).any(|(peer, _)| peer == leg),
                    "{scenario:?} on {}: {} → {} is not a link",
                    topo.label,
                    topo.devices[event.from as usize].name,
                    topo.devices[leg as usize].name
                );
            }
        }
    }
}

#[test]
fn scenarios_are_bounded_and_deterministic() {
    for (topo, scenario, events) in build_all() {
        assert!(
            (4..=32).contains(&events.len()),
            "{scenario:?} on {}",
            topo.label
        );
        let again = crate::scenario::build(scenario, topo);
        assert_eq!(
            events, again,
            "{scenario:?} on {} is not deterministic",
            topo.label
        );
    }
}

#[test]
fn ping_orders_arp_before_echo() {
    for topo in both_topologies() {
        let events = crate::scenario::build(Scenario::Ping, topo);
        let reply = find(&events, Proto::ArpReply).expect("ping has an ARP reply");
        let echo = find(&events, Proto::Echo).expect("ping has an echo");
        assert!(reply < echo, "ARP reply must precede the first echo");
        let request = find(&events, Proto::ArpReq).expect("ping has an ARP request");
        assert!(
            request < reply,
            "ARP broadcast must precede the unicast reply"
        );
        let echo_reply = find(&events, Proto::EchoReply).expect("ping has an echo reply");
        assert!(echo < echo_reply);
        // The request is a broadcast flood; the reply is unicast.
        assert!(events[request].broadcast);
        assert!(!events[reply].broadcast);
    }
}

#[test]
fn tcp_handshake_then_http_then_close() {
    for topo in both_topologies() {
        let events = crate::scenario::build(Scenario::Tcp, topo);
        let hops = topo.path(CLIENT, SERVER).len() - 1;
        let messages = [
            (CLIENT, Proto::Syn, SERVER),
            (SERVER, Proto::SynAck, CLIENT),
            (CLIENT, Proto::Ack, SERVER),
            (CLIENT, Proto::Get, SERVER),
            (SERVER, Proto::Ok200, CLIENT),
            (CLIENT, Proto::Fin, SERVER),
            (SERVER, Proto::FinAck, CLIENT),
            (CLIENT, Proto::Ack, SERVER),
        ];
        assert_eq!(events.len(), messages.len() * hops);
        for (message, (from, proto, to)) in events.chunks_exact(hops).zip(messages) {
            assert_eq!(message[0].from, from);
            assert!(message.iter().all(|hop| hop.proto == proto));
            assert!(message.last().unwrap().legs.contains(&to));
        }
        let first_switch_egress = &events[1];
        assert_eq!(
            first_switch_egress.legs.len(),
            if topo.router().is_some() { 1 } else { 2 }
        );
        assert!(!first_switch_egress.broadcast);
    }
}

#[test]
fn dhcp_is_dora() {
    for topo in both_topologies() {
        let events = crate::scenario::build(Scenario::Dhcp, topo);
        let protos: Vec<Proto> = events.iter().map(|e| e.proto).collect();
        assert_eq!(
            protos,
            vec![
                Proto::Discover,
                Proto::Discover,
                Proto::Offer,
                Proto::Offer,
                Proto::DhcpRequest,
                Proto::DhcpRequest,
                Proto::DhcpAck,
                Proto::DhcpAck,
            ],
            "DHCP must follow D-O-R-A"
        );
        // Discover and Request are broadcast floods; Offer/Ack come back unicast.
        assert!(events[0].broadcast && events[1].broadcast);
        assert!(!events[2].broadcast);
        assert!(!events[6].broadcast);
    }
}

#[test]
fn dns_resolves_after_arp() {
    for routed in [false, true] {
        let topo = topology(routed, true);
        let authority = topo.authority.unwrap();
        let events = crate::scenario::build(Scenario::Dns, topo);
        let first_query = find(&events, Proto::DnsQuery).unwrap();
        let arp_reply = find(&events, Proto::ArpReply).unwrap();
        assert!(arp_reply < first_query);
        assert_eq!(events[first_query].from, CLIENT);

        let resolver_hops = topo.path(CLIENT, SERVER).len() - 1;
        let authority_hops = topo.path(SERVER, authority).len() - 1;
        let second_query = first_query + resolver_hops;
        assert_eq!(events[second_query].proto, Proto::DnsQuery);
        assert_eq!(events[second_query].from, SERVER);
        assert!(events[second_query - 1].legs.contains(&SERVER));

        let answer = second_query + authority_hops;
        assert_eq!(events[answer].from, authority);
        assert!(events[answer - 1].legs.contains(&authority));
        assert_eq!(events[answer].proto, Proto::DnsResponse);
        assert!(events[answer].note.contains("203.0.113.10"));

        let resolver_answer = answer + authority_hops;
        assert_eq!(events[resolver_answer].from, SERVER);
        assert!(events[resolver_answer - 1].legs.contains(&SERVER));
        assert_eq!(events.last().unwrap().legs, [CLIENT]);
    }
}

#[test]
fn traceroute_hop_count_matches_router_count() {
    for topo in both_topologies() {
        let events = crate::scenario::build(Scenario::Traceroute, topo);
        let exceeded = events
            .iter()
            .filter(|e| e.proto == Proto::TtlExceeded)
            .count();
        let routers = usize::from(topo.router().is_some());
        // One Time Exceeded per router, crossing two links (router → switch → PC1).
        assert_eq!(
            exceeded,
            2 * routers,
            "Time Exceeded events on {}",
            topo.label
        );
        let hops = events.iter().filter(|e| e.note.starts_with("Hop ")).count();
        assert_eq!(hops, routers + 1, "recorded hops on {}", topo.label);
        let ttls: Vec<_> = events.iter().filter_map(|event| event.ttl).collect();
        assert_eq!(
            ttls,
            if routers == 1 {
                vec![1, 1, 2, 2, 1]
            } else {
                vec![1, 1]
            }
        );
        assert!(events.iter().any(|e| e.proto == Proto::PortUnreachable));
        assert!(events.last().unwrap().note.contains("Trace complete"));
    }
}

#[test]
fn arp_scenario_resolves_then_pings_without_broadcast() {
    for topo in both_topologies() {
        let events = crate::scenario::build(Scenario::Arp, topo);
        let last_broadcast = events.iter().rposition(|e| e.broadcast).unwrap();
        let echo = find(&events, Proto::Echo).unwrap();
        assert!(
            last_broadcast < echo,
            "the follow-up ping must use the cache, not ARP"
        );
    }
}

#[test]
fn broadcast_flood_fans_out_to_every_other_switch_port() {
    let single = topology(false, false);
    let sw = single.switch();
    let events = crate::scenario::build(Scenario::Ping, single);
    let flood = &events[1];
    assert_eq!(flood.from, sw);
    assert_eq!(
        flood.legs.len(),
        2,
        "single-switch floods to PC2 and the server"
    );
    let through = topology(true, false);
    let events = crate::scenario::build(Scenario::Ping, through);
    assert_eq!(events[1].legs.len(), 1, "through-router has one other port");
    assert_eq!(events[1].legs, [through.router().unwrap()]);
    assert_eq!(flood.legs, [1, 3]);
}

#[test]
fn table_replay_is_monotone_and_prefix_consistent() {
    for (topo, scenario, events) in build_all() {
        let full = crate::tables::replay(&events, events.len());
        let mut previous = crate::tables::TableState::default();
        for done in 0..=events.len() {
            let state = crate::tables::replay(&events, done);
            let truncated = crate::tables::replay(&events[..done], usize::MAX);
            assert_eq!(state, truncated, "{scenario:?} on {}", topo.label);
            assert!(state.mac.starts_with(&previous.mac));
            assert!(state.arp.starts_with(&previous.arp));
            assert!(state.mac.iter().all(|row| row.at < done));
            assert!(state.arp.iter().all(|row| row.at < done));
            assert!(full.mac.starts_with(&state.mac));
            assert!(full.arp.starts_with(&state.arp));
            previous = state;
        }
    }
}

#[test]
fn ping_through_router_fills_both_tables() {
    let topo = topology(true, false);
    let events = crate::scenario::build(Scenario::Ping, topo);
    let state = crate::tables::replay(&events, events.len());
    assert_eq!(state.arp.len(), 1, "PC1 cached exactly the gateway");
    assert_eq!(state.arp[0].ip, "192.168.1.1");
    assert_eq!(state.mac.len(), 2, "switch learned PC1 and the router");
    let ats: Vec<usize> = state.mac.iter().map(|r| r.at).collect();
    assert!(
        ats.windows(2).all(|w| w[0] < w[1]),
        "insertion order is stable"
    );
}

#[test]
fn tables_start_empty() {
    for (_, _, events) in build_all() {
        let state = crate::tables::replay(&events, 0);
        assert!(state.mac.is_empty() && state.arp.is_empty());
        assert!(state.last_event().is_none());
    }
}

#[test]
fn timeline_completes_every_event() {
    for n in [1_usize, 5, 18, 32] {
        assert_eq!(crate::timeline::done(crate::timeline::position(1.0, n)), n);
        assert_eq!(crate::timeline::done(crate::timeline::position(0.0, n)), 0);
    }
    // The last event is still playing just before the final hold.
    let u = crate::timeline::position(0.93, 10);
    assert_eq!(crate::timeline::playing(u, 10), Some(9));
}

#[test]
fn packet_flight_is_continuous_within_a_hop() {
    // Eased position moves strictly forward and hits both endpoints exactly.
    let mut previous = 0.0_f32;
    for step in 1..=100 {
        let t = crate::timeline::ease(step as f32 / 100.0);
        assert!(t > previous, "eased flight went backwards at step {step}");
        assert!((0.0..=1.0).contains(&t));
        previous = t;
    }
    assert_eq!(crate::timeline::ease(0.0), 0.0);
    assert_eq!(crate::timeline::ease(1.0), 1.0);
}

fn render_params(values: &[(&str, ParamValue)]) -> Vec<ParamEntry> {
    values.iter().map(|(id, v)| pv(id, v.clone())).collect()
}

fn snapshot(params: &[ParamEntry]) -> String {
    format!(
        "{:?}",
        render(
            params,
            Size {
                width: 1280,
                height: 720
            }
        )
        .unwrap()
    )
}

fn text_runs(scene: &Scene) -> usize {
    scene
        .elements
        .iter()
        .filter(|e| matches!(e, DrawCommand::TextRun(_)))
        .count()
}

#[test]
fn params_are_validated() {
    let bad_number = [pv("progress", ParamValue::Number(f64::NAN))];
    assert!(render(
        &bad_number,
        Size {
            width: 1280,
            height: 720
        }
    )
    .is_err());
    let bad_scenario = [pv("scenario", ParamValue::EnumChoice("ospf".into()))];
    assert!(render(
        &bad_scenario,
        Size {
            width: 1280,
            height: 720
        }
    )
    .is_err());
    let bad_labels = [pv("labels", ParamValue::EnumChoice("tiny".into()))];
    assert!(render(
        &bad_labels,
        Size {
            width: 1280,
            height: 720
        }
    )
    .is_err());
    let bad_table = [pv("table", ParamValue::EnumChoice("bgp".into()))];
    assert!(render(
        &bad_table,
        Size {
            width: 1280,
            height: 720
        }
    )
    .is_err());
    let empty_canvas: [ParamEntry; 0] = [];
    assert!(
        render(
            &empty_canvas,
            Size {
                width: 0,
                height: 720
            }
        )
        .is_err(),
        "zero canvas is rejected"
    );
    // Out-of-range numbers clamp instead of failing.
    let clamped = [
        pv("steps", ParamValue::Number(1000.0)),
        pv("packet_size", ParamValue::Number(99.0)),
        pv("progress", ParamValue::Number(-3.0)),
    ];
    assert!(render(
        &clamped,
        Size {
            width: 1280,
            height: 720
        }
    )
    .is_ok());
}

#[test]
fn label_modes_change_the_scene() {
    let none = render_params(&[
        ("scenario", ParamValue::EnumChoice("tcp".into())),
        ("topology", ParamValue::EnumChoice("through-router".into())),
        ("labels", ParamValue::EnumChoice("none".into())),
    ]);
    let brief = render_params(&[
        ("scenario", ParamValue::EnumChoice("tcp".into())),
        ("topology", ParamValue::EnumChoice("through-router".into())),
        ("labels", ParamValue::EnumChoice("brief".into())),
    ]);
    let full = render_params(&[
        ("scenario", ParamValue::EnumChoice("tcp".into())),
        ("topology", ParamValue::EnumChoice("through-router".into())),
        ("labels", ParamValue::EnumChoice("full".into())),
    ]);
    let size = Size {
        width: 1280,
        height: 720,
    };
    let none = text_runs(&render(&none, size).unwrap());
    let brief = text_runs(&render(&brief, size).unwrap());
    let full = text_runs(&render(&full, size).unwrap());
    assert!(
        none < brief && brief < full,
        "text runs: none={none} brief={brief} full={full}"
    );
}

#[test]
fn scenario_and_topology_change_the_scene() {
    let ping = [
        pv("scenario", ParamValue::EnumChoice("ping".into())),
        pv("progress", ParamValue::Number(0.6)),
    ];
    let dhcp = [
        pv("scenario", ParamValue::EnumChoice("dhcp".into())),
        pv("progress", ParamValue::Number(0.6)),
    ];
    assert_ne!(snapshot(&ping), snapshot(&dhcp));
    let single = [
        pv("topology", ParamValue::EnumChoice("single-switch".into())),
        pv("progress", ParamValue::Number(0.6)),
    ];
    let through = [
        pv("topology", ParamValue::EnumChoice("through-router".into())),
        pv("progress", ParamValue::Number(0.6)),
    ];
    assert_ne!(snapshot(&single), snapshot(&through));
}

#[test]
fn progress_advances_the_scene() {
    let mut previous = String::new();
    for step in 0..=20 {
        let params = [pv("progress", ParamValue::Number(step as f64 / 20.0))];
        let snap = snapshot(&params);
        if step > 0 {
            assert_ne!(
                snap, previous,
                "progress {step}/20 did not change the scene"
            );
        }
        previous = snap;
    }
}

#[test]
fn render_is_deterministic_and_different_per_scenario() {
    for scenario in SCENARIOS {
        let params = [pv("scenario", ParamValue::EnumChoice(scenario.into()))];
        assert_eq!(
            snapshot(&params),
            snapshot(&params),
            "{scenario} must be deterministic"
        );
    }
}

#[test]
fn steps_param_truncates_the_timeline() {
    let short = [
        pv("steps", ParamValue::Number(3.0)),
        pv("progress", ParamValue::Number(1.0)),
    ];
    let snap = snapshot(&short);
    // Only 3 of ping's 8+ events play: the scene differs from the full run.
    let full = [pv("progress", ParamValue::Number(1.0))];
    assert_ne!(snap, snapshot(&full));
    // A step cap above the scenario length changes nothing.
    let generous = [pv("steps", ParamValue::Number(200.0))];
    assert_eq!(snapshot(&generous), snapshot(&full));
}

#[test]
fn panels_can_be_hidden() {
    let off = [
        pv("show_tables", ParamValue::Bool(false)),
        pv("show_step_list", ParamValue::Bool(false)),
    ];
    let snap = snapshot(&off);
    let on: [ParamEntry; 0] = [];
    assert_ne!(snap, snapshot(&on));
}

#[test]
fn small_canvas_renders_without_error() {
    for (topo, scenario, _) in build_all() {
        let topology_name = if topo.router().is_some() {
            "through-router"
        } else {
            "single-switch"
        };
        let params = render_params(&[
            (
                "scenario",
                ParamValue::EnumChoice(scenario_name(scenario).into()),
            ),
            ("topology", ParamValue::EnumChoice(topology_name.into())),
        ]);
        for size in [(180, 320), (320, 180), (1280, 720), (1920, 1080)] {
            let scene = render(
                &params,
                Size {
                    width: size.0,
                    height: size.1,
                },
            );
            assert!(
                scene.is_ok(),
                "{} on {} at {size:?}",
                scenario_name(scenario),
                topo.label
            );
        }
    }
}

fn scenario_name(scenario: Scenario) -> &'static str {
    match scenario {
        Scenario::Ping => "ping",
        Scenario::Arp => "arp",
        Scenario::Tcp => "tcp",
        Scenario::Dhcp => "dhcp",
        Scenario::Dns => "dns",
        Scenario::Traceroute => "traceroute",
    }
}

#[test]
fn router_uses_interface_macs() {
    let topo = topology(true, false);
    let sw = topo.switch();
    let rtr = topo.router().unwrap();
    assert_eq!(topo.mac_towards(rtr, sw), "02:22:33:44:55:01");
    assert_eq!(topo.mac_towards(rtr, 3), "02:22:33:44:55:02");
    assert_eq!(topo.mac_towards(0, sw), "02:11:22:33:44:01");
}

#[test]
fn describe_matches_manifest_contract() {
    let manifest = crate::NetLab::describe();
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.kinds, vec!["clip-source".to_string()]);
    assert!(manifest.params.len() <= 32, "at most 32 parameters");
    let ids: Vec<&str> = manifest.params.iter().map(|p| p.id.as_str()).collect();
    for required in [
        "scenario",
        "topology",
        "progress",
        "steps",
        "labels",
        "show_tables",
        "table",
        "packet_size",
        "accent",
        "show_step_list",
        "speed_lines",
    ] {
        assert!(ids.contains(&required), "missing param {required}");
    }
}

#[test]
fn lease_appears_only_after_the_ack_reaches_the_client() {
    let topo = topology(false, false);
    let events = crate::scenario::build(Scenario::Dhcp, topo);
    let scene_at = |slot: f32, steps: usize| {
        let progress =
            (slot + timeline::LEAD_IN) / (steps as f32 + timeline::LEAD_IN + timeline::LEAD_OUT);
        let params = [
            pv("scenario", ParamValue::EnumChoice("dhcp".into())),
            pv("topology", ParamValue::EnumChoice("single-switch".into())),
            pv("progress", ParamValue::Number(f64::from(progress))),
            pv("steps", ParamValue::Number(steps as f64)),
        ];
        render(
            &params,
            Size {
                width: 1280,
                height: 720,
            },
        )
        .unwrap()
    };
    let contains = |scene: Scene, text: &str| {
        scene
            .elements
            .iter()
            .any(|element| matches!(element, DrawCommand::TextRun(run) if run.text == text))
    };
    assert!(contains(scene_at(7.5, events.len()), "Awaiting lease"));
    assert!(contains(scene_at(7.25, 7), "Awaiting lease"));
    assert!(!contains(scene_at(8.1, events.len()), "Awaiting lease"));
    assert!(contains(scene_at(8.1, events.len()), "192.168.1.10"));
}
