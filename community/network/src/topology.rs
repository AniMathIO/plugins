//! Teaching topologies, link paths and layout anchors.
//!
//! Everything is static data; the drawing layer scales it into the free part of the
//! canvas, so a render is resolution-independent by construction.

use std::collections::VecDeque;

pub const CLIENT: u8 = 0;
pub const PEER: u8 = 1;
pub const SERVER: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceKind {
    Host,
    Switch,
    Router,
    Server,
}

#[derive(Clone, Copy)]
pub struct Device {
    pub name: &'static str,
    pub kind: DeviceKind,
    /// Management/primary IPv4 address, `""` for the layer-2-only switch.
    pub ip: &'static str,
    pub mac: &'static str,
}

#[derive(Clone, Copy)]
pub struct Link {
    pub a: u8,
    /// Port name on device `a`.
    pub a_port: &'static str,
    pub b: u8,
    /// Port name on device `b`.
    pub b_port: &'static str,
}

#[derive(Clone, Copy)]
pub struct RouteRow {
    pub destination: &'static str,
    pub kind: &'static str,
    pub interface: &'static str,
}

pub struct Topology {
    pub label: &'static str,
    pub devices: &'static [Device],
    pub links: &'static [Link],
    /// The authoritative DNS service, present in the DNS lesson.
    pub authority: Option<u8>,
    /// Static routing-table rows of the router, empty without one.
    pub routes: &'static [RouteRow],
    /// Layout anchor per device as fractions of the free diagram rectangle.
    pub anchors: &'static [(f32, f32)],
    /// True when the layout draws labels above the device box.
    pub label_above: &'static [bool],
}

const PC1_MAC: &str = "02:11:22:33:44:01";
const PC2_MAC: &str = "02:11:22:33:44:02";
const SRV_MAC: &str = "02:11:22:33:44:0A";
const SW_MAC: &str = "02:11:22:33:00:0B";
const RTR_MAC0: &str = "02:22:33:44:55:01";
const RTR_MAC1: &str = "02:22:33:44:55:02";
const RTR_MAC2: &str = "02:22:33:44:55:03";

/// `single-switch`: PC1, PC2 and a server on one LAN behind one switch.
const SINGLE_SWITCH: Topology = Topology {
    label: "Single LAN · 192.168.1.0/24",
    devices: &[
        Device {
            name: "PC1",
            kind: DeviceKind::Host,
            ip: "192.168.1.10",
            mac: PC1_MAC,
        },
        Device {
            name: "PC2",
            kind: DeviceKind::Host,
            ip: "192.168.1.20",
            mac: PC2_MAC,
        },
        Device {
            name: "Switch",
            kind: DeviceKind::Switch,
            ip: "",
            mac: SW_MAC,
        },
        Device {
            name: "Server",
            kind: DeviceKind::Server,
            ip: "192.168.1.50",
            mac: SRV_MAC,
        },
    ],
    links: &[
        Link {
            a: 0,
            a_port: "Eth0",
            b: 2,
            b_port: "Fa0/1",
        },
        Link {
            a: 1,
            a_port: "Eth0",
            b: 2,
            b_port: "Fa0/2",
        },
        Link {
            a: 3,
            a_port: "Eth0",
            b: 2,
            b_port: "Fa0/3",
        },
    ],
    authority: None,
    routes: &[],
    anchors: &[(0.16, 0.68), (0.84, 0.68), (0.50, 0.68), (0.50, 0.30)],
    label_above: &[false, false, false, true],
};

/// `through-router`: PC1 — switch — router — server across two subnets.
const THROUGH_ROUTER: Topology = Topology {
    label: "Routed LANs · 192.168.1.0/24 and 10.0.0.0/24",
    devices: &[
        Device {
            name: "PC1",
            kind: DeviceKind::Host,
            ip: "192.168.1.10",
            mac: PC1_MAC,
        },
        Device {
            name: "Switch",
            kind: DeviceKind::Switch,
            ip: "",
            mac: SW_MAC,
        },
        Device {
            name: "Router",
            kind: DeviceKind::Router,
            ip: "192.168.1.1",
            mac: RTR_MAC0,
        },
        Device {
            name: "Server",
            kind: DeviceKind::Server,
            ip: "10.0.0.50",
            mac: SRV_MAC,
        },
    ],
    links: &[
        Link {
            a: 0,
            a_port: "Eth0",
            b: 1,
            b_port: "Fa0/1",
        },
        Link {
            a: 1,
            a_port: "Fa0/2",
            b: 2,
            b_port: "Gi0/0",
        },
        Link {
            a: 2,
            a_port: "Gi0/1",
            b: 3,
            b_port: "Eth0",
        },
    ],
    authority: None,
    routes: &[
        RouteRow {
            destination: "192.168.1.0/24",
            kind: "Connected",
            interface: "Gi0/0",
        },
        RouteRow {
            destination: "10.0.0.0/24",
            kind: "Connected",
            interface: "Gi0/1",
        },
    ],
    anchors: &[(0.10, 0.55), (0.37, 0.55), (0.63, 0.55), (0.90, 0.55)],
    label_above: &[false, false, false, false],
};

static LOCAL_DNS: Topology = Topology {
    devices: &[
        SINGLE_SWITCH.devices[0],
        Device {
            name: "Authority",
            kind: DeviceKind::Server,
            ..SINGLE_SWITCH.devices[1]
        },
        SINGLE_SWITCH.devices[2],
        Device {
            name: "Resolver",
            ..SINGLE_SWITCH.devices[3]
        },
    ],
    authority: Some(PEER),
    ..SINGLE_SWITCH
};

static ROUTED_DNS: Topology = Topology {
    label: "Routed LANs · client, resolver and authority",
    devices: &[
        THROUGH_ROUTER.devices[0],
        THROUGH_ROUTER.devices[1],
        THROUGH_ROUTER.devices[2],
        Device {
            name: "Resolver",
            ..THROUGH_ROUTER.devices[3]
        },
        Device {
            name: "Authority",
            kind: DeviceKind::Server,
            ip: "10.0.1.53",
            mac: "02:11:22:33:44:0B",
        },
    ],
    links: &[
        THROUGH_ROUTER.links[0],
        THROUGH_ROUTER.links[1],
        THROUGH_ROUTER.links[2],
        Link {
            a: 2,
            a_port: "Gi0/2",
            b: 4,
            b_port: "Eth0",
        },
    ],
    routes: &[
        THROUGH_ROUTER.routes[0],
        THROUGH_ROUTER.routes[1],
        RouteRow {
            destination: "10.0.1.0/24",
            kind: "Connected",
            interface: "Gi0/2",
        },
    ],
    anchors: &[
        (0.12, 0.64),
        (0.37, 0.64),
        (0.62, 0.64),
        (0.88, 0.64),
        (0.88, 0.28),
    ],
    label_above: &[false, false, false, false, true],
    authority: Some(4),
};

impl Topology {
    pub fn switch(&self) -> u8 {
        self.device_of_kind(DeviceKind::Switch)
            .expect("each preset has a switch")
    }

    pub fn router(&self) -> Option<u8> {
        self.device_of_kind(DeviceKind::Router)
    }

    fn device_of_kind(&self, kind: DeviceKind) -> Option<u8> {
        self.devices
            .iter()
            .position(|device| device.kind == kind)
            .map(|index| index as u8)
    }

    pub fn neighbors(&self, device: u8) -> impl Iterator<Item = (u8, &'static str)> + '_ {
        self.links.iter().filter_map(move |link| {
            if link.a == device {
                Some((link.b, link.a_port))
            } else if link.b == device {
                Some((link.a, link.b_port))
            } else {
                None
            }
        })
    }

    pub fn port_towards(&self, device: u8, peer: u8) -> &'static str {
        self.neighbors(device)
            .find_map(|(neighbor, port)| (neighbor == peer).then_some(port))
            .expect("adjacent nodes in a preset path")
    }

    pub fn mac_towards(&self, device: u8, peer: u8) -> &'static str {
        let device_info = &self.devices[usize::from(device)];
        if device_info.kind != DeviceKind::Router {
            return device_info.mac;
        }
        match self.port_towards(device, peer) {
            "Gi0/0" => RTR_MAC0,
            "Gi0/1" => RTR_MAC1,
            "Gi0/2" => RTR_MAC2,
            _ => unreachable!("router port in a preset"),
        }
    }

    /// All presets are small connected trees, so each pair has one physical path.
    pub fn path(&self, from: u8, to: u8) -> Vec<u8> {
        let mut parents = vec![None; self.devices.len()];
        parents[usize::from(from)] = Some(from);
        let mut queue = VecDeque::from([from]);
        while let Some(node) = queue.pop_front() {
            if node == to {
                let mut path = vec![node];
                while let Some(&last) = path.last() {
                    if last == from {
                        path.reverse();
                        return path;
                    }
                    path.push(parents[usize::from(last)].expect("visited node has a parent"));
                }
            }
            for (neighbor, _) in self.neighbors(node) {
                if parents[usize::from(neighbor)].is_none() {
                    parents[usize::from(neighbor)] = Some(node);
                    queue.push_back(neighbor);
                }
            }
        }
        unreachable!("preset connects all devices")
    }
}

pub fn topology(through_router: bool, dns: bool) -> &'static Topology {
    match (through_router, dns) {
        (false, false) => &SINGLE_SWITCH,
        (true, false) => &THROUGH_ROUTER,
        (false, true) => &LOCAL_DNS,
        (true, true) => &ROUTED_DNS,
    }
}
