//! Protocol scenarios as ordered, per-hop event lists — the heart of the plugin.
//!
//! Every event moves one packet across exactly one link hop (`from` → `legs`); a
//! broadcast hop fans the frame out to every other port of the switch at once, the
//! way a real switch floods. Builders are pure functions of the topology choice and
//! follow the CCNA-textbook sequencing studied in the MIT netsim projects (see NOTICE).

use super::tables::TableUpdate;
use super::topology::{Topology, CLIENT, PEER, SERVER};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Proto {
    ArpReq,
    ArpReply,
    Echo,
    EchoReply,
    TtlExceeded,
    PortUnreachable,
    Syn,
    SynAck,
    Ack,
    Get,
    Ok200,
    Fin,
    FinAck,
    Discover,
    Offer,
    DhcpRequest,
    DhcpAck,
    DnsQuery,
    DnsResponse,
    Probe,
}

impl Proto {
    /// Text painted on the packet in `labels = brief` mode.
    #[must_use]
    pub fn short(self) -> &'static str {
        match self {
            Proto::ArpReq | Proto::ArpReply => "ARP",
            Proto::Echo => "Echo",
            Proto::EchoReply => "Reply",
            Proto::TtlExceeded => "Expired",
            Proto::PortUnreachable => "Port",
            Proto::Syn => "SYN",
            Proto::SynAck => "SYN-ACK",
            Proto::Ack => "ACK",
            Proto::Get => "GET",
            Proto::Ok200 => "200 OK",
            Proto::Fin => "FIN",
            Proto::FinAck => "FIN-ACK",
            Proto::Discover => "DISCOVER",
            Proto::Offer => "OFFER",
            Proto::DhcpRequest => "REQUEST",
            Proto::DhcpAck => "ACK",
            Proto::DnsQuery => "DNS?",
            Proto::DnsResponse => "DNS!",
            Proto::Probe => "UDP",
        }
    }

    /// Text painted on the packet in `labels = full` mode.
    #[must_use]
    pub fn full(self) -> &'static str {
        match self {
            Proto::ArpReq => "ARP Request",
            Proto::ArpReply => "ARP Reply",
            Proto::Echo => "Echo Request",
            Proto::EchoReply => "Echo Reply",
            Proto::TtlExceeded => "Time Exceeded",
            Proto::PortUnreachable => "Port Unreachable",
            Proto::Syn => "TCP SYN",
            Proto::SynAck => "TCP SYN-ACK",
            Proto::Ack => "TCP ACK",
            Proto::Get => "HTTP GET",
            Proto::Ok200 => "HTTP 200 OK",
            Proto::Fin => "TCP FIN",
            Proto::FinAck => "TCP FIN-ACK",
            Proto::Discover => "DHCP Discover",
            Proto::Offer => "DHCP Offer",
            Proto::DhcpRequest => "DHCP Request",
            Proto::DhcpAck => "DHCP ACK",
            Proto::DnsQuery => "DNS Query",
            Proto::DnsResponse => "DNS Response",
            Proto::Probe => "UDP Probe",
        }
    }

    /// Packet fill color, stable per protocol family (orange ARP, blue ICMP,
    /// green TCP/HTTP, purple DHCP, pink DNS, brown probes, red errors).
    #[must_use]
    pub fn rgba(self) -> (u8, u8, u8) {
        match self {
            Proto::ArpReq => (255, 167, 38),
            Proto::ArpReply => (255, 213, 79),
            Proto::Echo => (66, 165, 245),
            Proto::EchoReply => (38, 198, 218),
            Proto::TtlExceeded | Proto::PortUnreachable => (239, 83, 80),
            Proto::Syn => (67, 160, 71),
            Proto::SynAck => (102, 187, 106),
            Proto::Ack => (129, 199, 132),
            Proto::Get => (139, 195, 74),
            Proto::Ok200 => (104, 159, 56),
            Proto::Fin | Proto::FinAck => (156, 204, 101),
            Proto::Discover | Proto::DhcpRequest => (126, 87, 194),
            Proto::Offer | Proto::DhcpAck => (149, 117, 205),
            Proto::DnsQuery => (236, 64, 122),
            Proto::DnsResponse => (240, 98, 146),
            Proto::Probe => (141, 110, 99),
        }
    }
}

/// One packet crossing one link hop. Broadcast hops fan out to several legs at once.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Event {
    pub from: u8,
    pub legs: Vec<u8>,
    pub proto: Proto,
    pub broadcast: bool,
    pub ttl: Option<u8>,
    /// Step-list explanation of this hop.
    pub note: String,
    /// Table entry learned when this hop completes.
    pub table: Option<TableUpdate>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scenario {
    Ping,
    Arp,
    Tcp,
    Dhcp,
    Dns,
    Traceroute,
}

pub const SCENARIOS: [&str; 6] = ["ping", "arp", "tcp", "dhcp", "dns", "traceroute"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TableKind {
    Auto,
    Mac,
    Arp,
    Route,
}

pub const TABLES: [&str; 4] = ["auto", "mac", "arp", "route"];

impl Scenario {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ping" => Some(Self::Ping),
            "arp" => Some(Self::Arp),
            "tcp" => Some(Self::Tcp),
            "dhcp" => Some(Self::Dhcp),
            "dns" => Some(Self::Dns),
            "traceroute" => Some(Self::Traceroute),
            _ => None,
        }
    }

    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Ping => "ICMP Ping",
            Self::Arp => "ARP Resolution",
            Self::Tcp => "TCP & HTTP",
            Self::Dhcp => "DHCP DORA",
            Self::Dns => "DNS Lookup",
            Self::Traceroute => "Traceroute",
        }
    }

    /// Table the `auto` panel choice shows for this scenario.
    #[must_use]
    pub fn auto_table(self) -> TableKind {
        match self {
            Self::Ping | Self::Dns => TableKind::Arp,
            Self::Arp | Self::Tcp | Self::Dhcp => TableKind::Mac,
            Self::Traceroute => TableKind::Route,
        }
    }
}

/// Appends hops in wire order. Switch ingress learns a source, and a switch
/// egress floods when the destination MAC has yet to be learned.
struct Exchange<'a> {
    topo: &'a Topology,
    switch: u8,
    learned: Vec<u8>,
    events: Vec<Event>,
}

impl Exchange<'_> {
    fn hop(&mut self, from: u8, legs: Vec<u8>, proto: Proto, broadcast: bool, note: String) {
        let table = if legs == [self.switch] && !self.learned.contains(&from) {
            self.learned.push(from);
            Some(TableUpdate::Mac {
                port: self.topo.port_towards(self.switch, from),
                mac: self.topo.mac_towards(from, self.switch),
                device: from,
            })
        } else {
            None
        };
        self.events.push(Event {
            from,
            legs,
            proto,
            broadcast,
            ttl: None,
            note,
            table,
        });
    }

    fn flood(&self, ingress: u8) -> Vec<u8> {
        self.topo
            .neighbors(self.switch)
            .map(|(node, _)| node)
            .filter(|&node| node != ingress)
            .collect()
    }

    fn broadcast_on_lan(&mut self, from: u8, proto: Proto, send: String, flood: &str) {
        self.hop(from, vec![self.switch], proto, true, send);
        self.hop(self.switch, self.flood(from), proto, true, flood.into());
    }

    fn send(&mut self, from: u8, to: u8, proto: Proto, note: impl Into<String>) -> &mut [Event] {
        let first = self.events.len();
        let path = self.topo.path(from, to);
        let mut note = note.into();
        for (index, pair) in path.windows(2).enumerate() {
            let (sender, receiver) = (pair[0], pair[1]);
            if sender == self.switch && !self.learned.contains(&receiver) {
                // The underlying frame remains unicast. Only its switch egress
                // is duplicated, so there is no broadcast wave.
                let legs = self.flood(path[index - 1]);
                let explanation = "Unknown MAC: switch floods the frame";
                self.hop(sender, legs, proto, false, explanation.into());
            } else {
                self.hop(
                    sender,
                    vec![receiver],
                    proto,
                    false,
                    std::mem::take(&mut note),
                );
            }
        }
        &mut self.events[first..]
    }

    fn last(&mut self) -> &mut Event {
        self.events.last_mut().expect("message contains a hop")
    }

    fn resolve_on_lan(&mut self, target: u8) {
        let target_ip = self.topo.devices[usize::from(target)].ip;
        let mac = self.topo.mac_towards(target, self.switch);
        self.broadcast_on_lan(
            CLIENT,
            Proto::ArpReq,
            format!("Who has {target_ip}? Tell PC1"),
            "Switch floods to its other ports",
        );
        self.send(target, CLIENT, Proto::ArpReply, format!("My MAC is {mac}"));
        let arrival = self.last();
        arrival.note = format!("PC1 caches the MAC for {target_ip}");
        arrival.table = Some(TableUpdate::Arp {
            device: CLIENT,
            ip: target_ip,
            mac,
        });
    }

    fn ping(&mut self) {
        let router = self.topo.router();
        self.resolve_on_lan(router.unwrap_or(SERVER));
        let target_ip = self.topo.devices[usize::from(SERVER)].ip;
        if let Some(router) = router {
            self.send(
                CLIENT,
                router,
                Proto::Echo,
                format!("Ping {target_ip} via the gateway"),
            );
            self.hop(
                router,
                vec![SERVER],
                Proto::ArpReq,
                true,
                format!("Router ARPs for {target_ip} on Gi0/1"),
            );
            self.send(SERVER, router, Proto::ArpReply, "Server supplies its MAC");
            self.send(
                router,
                SERVER,
                Proto::Echo,
                "Router forwards the Echo Request",
            );
        } else {
            self.send(
                CLIENT,
                SERVER,
                Proto::Echo,
                format!("Ping {target_ip} on the same LAN"),
            );
        }
        self.echo_reply(SERVER);
    }

    fn arp(&mut self) {
        let target = self.topo.router().unwrap_or(PEER);
        self.resolve_on_lan(target);
        self.send(
            CLIENT,
            target,
            Proto::Echo,
            "Echo Request uses the cached MAC",
        );
        self.echo_reply(target);
    }

    fn echo_reply(&mut self, target: u8) {
        self.send(
            target,
            CLIENT,
            Proto::EchoReply,
            "Destination sends an Echo Reply",
        );
        self.last().note = "PC1 receives the Echo Reply".into();
    }

    fn tcp(&mut self) {
        // The endpoints already know their next-hop MACs. The first SYN can
        // still flood while the switch's own forwarding table is empty.
        self.send(CLIENT, SERVER, Proto::Syn, "PC1 opens a TCP connection");
        self.send(
            SERVER,
            CLIENT,
            Proto::SynAck,
            "Server acknowledges and sends its SYN",
        );
        self.send(
            CLIENT,
            SERVER,
            Proto::Ack,
            "PC1 acknowledges the server's SYN",
        );
        self.last().note = "Server receives ACK: handshake complete".into();
        self.send(CLIENT, SERVER, Proto::Get, "GET /index.html");
        self.send(SERVER, CLIENT, Proto::Ok200, "Server returns the page");
        self.send(CLIENT, SERVER, Proto::Fin, "PC1 finishes sending");
        self.send(
            SERVER,
            CLIENT,
            Proto::FinAck,
            "Server acknowledges FIN and sends its own FIN",
        );
        self.send(
            CLIENT,
            SERVER,
            Proto::Ack,
            "PC1 acknowledges the server's FIN",
        );
    }

    fn dhcp(&mut self) {
        let router = self.topo.router();
        let server = router.unwrap_or(SERVER);
        let offer = if router.is_some() {
            "Offers 192.168.1.10/24, gateway 192.168.1.1"
        } else {
            "Offers 192.168.1.10/24 on the local LAN"
        };
        self.broadcast_on_lan(
            CLIENT,
            Proto::Discover,
            "PC1 broadcasts to find a DHCP server".into(),
            "Switch floods Discover on the LAN",
        );
        self.send(server, CLIENT, Proto::Offer, offer);
        self.broadcast_on_lan(
            CLIENT,
            Proto::DhcpRequest,
            "PC1 broadcasts its selected offer".into(),
            "All DHCP servers hear the choice",
        );
        self.send(
            server,
            CLIENT,
            Proto::DhcpAck,
            "Selected server grants the lease",
        );
        self.last().note = "PC1 configures 192.168.1.10/24".into();
    }

    fn dns(&mut self) {
        let authority = self.topo.authority.expect("DNS preset has an authority");
        self.resolve_on_lan(self.topo.router().unwrap_or(SERVER));
        self.send(
            CLIENT,
            SERVER,
            Proto::DnsQuery,
            "PC1 asks its resolver for example.test",
        );
        self.send(
            SERVER,
            authority,
            Proto::DnsQuery,
            "Cache miss: resolver asks the authority",
        );
        self.send(
            authority,
            SERVER,
            Proto::DnsResponse,
            "Authority answers: 203.0.113.10",
        );
        self.send(
            SERVER,
            CLIENT,
            Proto::DnsResponse,
            "Resolver caches the answer and replies",
        );
        self.last().note = "PC1 receives the address 203.0.113.10".into();
    }

    fn traceroute(&mut self) {
        let mut destination_ttl = 1;
        if let Some(router) = self.topo.router() {
            let expired = self.send(CLIENT, router, Proto::Probe, "Probe 1 starts with TTL 1");
            for step in expired.iter_mut() {
                step.ttl = Some(1);
            }
            expired.last_mut().unwrap().note = "Router drops the probe: TTL expires".into();
            self.send(
                router,
                CLIENT,
                Proto::TtlExceeded,
                "Router sends ICMP Time Exceeded",
            );
            self.last().note = "Hop 1 recorded: 192.168.1.1".into();
            destination_ttl = 2;
        }
        let router = self.topo.router();
        let probe = self.send(
            CLIENT,
            SERVER,
            Proto::Probe,
            format!("Probe starts with TTL {destination_ttl}"),
        );
        for step in probe {
            step.ttl = Some(destination_ttl - u8::from(router == Some(step.from)));
            if router == Some(step.from) {
                step.note = "Router forwards with TTL 1".into();
            } else if router.is_none() && step.from != CLIENT && step.legs.len() == 1 {
                step.note = "Server receives the probe with TTL 1".into();
            }
        }
        self.send(
            SERVER,
            CLIENT,
            Proto::PortUnreachable,
            "Server returns ICMP Port Unreachable",
        );
        let target_ip = self.topo.devices[usize::from(SERVER)].ip;
        self.last().note = format!("Hop {destination_ttl}: {target_ip}. Trace complete");
    }
}

pub fn build(scenario: Scenario, topo: &Topology) -> Vec<Event> {
    let mut exchange = Exchange {
        topo,
        switch: topo.switch(),
        learned: Vec::new(),
        events: Vec::new(),
    };
    match scenario {
        Scenario::Ping => exchange.ping(),
        Scenario::Arp => exchange.arp(),
        Scenario::Tcp => exchange.tcp(),
        Scenario::Dhcp => exchange.dhcp(),
        Scenario::Dns => exchange.dns(),
        Scenario::Traceroute => exchange.traceroute(),
    }
    exchange.events
}
