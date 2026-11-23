//! Tables (switch MAC table, ARP caches) recomputed as a pure replay of events.
//!
//! `replay(events, done)` returns the state after the first `done` events completed;
//! rows keep their insertion order and the event index that inserted them, so the
//! panel can highlight what the last event learned.

use super::scenario::Event;

/// A learning event applied when its frame reaches the next device.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TableUpdate {
    /// A switch learned the source MAC of a frame entering on `port`.
    Mac {
        port: &'static str,
        mac: &'static str,
        device: u8,
    },
    /// A host cached an IP↔MAC pair in its ARP table.
    Arp {
        device: u8,
        ip: &'static str,
        mac: &'static str,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct MacRow {
    pub port: &'static str,
    pub mac: &'static str,
    pub device: u8,
    pub at: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ArpRow {
    pub device: u8,
    pub ip: &'static str,
    pub mac: &'static str,
    pub at: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TableState {
    pub mac: Vec<MacRow>,
    pub arp: Vec<ArpRow>,
}

impl TableState {
    /// Event index of the most recent row insert (exercised by the unit tests).
    #[cfg(test)]
    #[must_use]
    pub fn last_event(&self) -> Option<usize> {
        let mac = self.mac.last().map(|r| r.at);
        let arp = self.arp.last().map(|r| r.at);
        match (mac, arp) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }
}

#[must_use]
pub fn replay(events: &[Event], done: usize) -> TableState {
    let mut state = TableState::default();
    for (index, event) in events.iter().take(done).enumerate() {
        let Some(update) = &event.table else { continue };
        match update {
            TableUpdate::Mac { port, mac, device } => {
                let (port, mac, device) = (*port, *mac, *device);
                if !state
                    .mac
                    .iter()
                    .any(|row| row.mac == mac && row.device == device)
                {
                    state.mac.push(MacRow {
                        port,
                        mac,
                        device,
                        at: index,
                    });
                }
            }
            TableUpdate::Arp { device, ip, mac } => {
                let (device, ip, mac) = (*device, *ip, *mac);
                if !state
                    .arp
                    .iter()
                    .any(|row| row.device == device && row.ip == ip)
                {
                    state.arp.push(ArpRow {
                        device,
                        ip,
                        mac,
                        at: index,
                    });
                }
            }
        }
    }
    state
}
