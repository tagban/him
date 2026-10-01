//! Client side of the Hotline Instant Messaging extension, written from
//! fogWraith's specification (github.com/fogWraith/Hotline, Docs/IM and
//! Docs/Protocol).

pub mod client;
pub mod emoticons;
pub mod frame;
pub mod history;
pub mod hope;
pub mod icon;
pub mod info;
pub mod media;
pub mod messaging;
pub mod text;
pub mod tls;
pub mod tracker;
pub mod transfer;
pub mod wire;

#[cfg(any(test, feature = "mock-server"))]
pub mod mock;

pub use history::{HistoryEntry, HistoryPage};
pub use media::{MediaLimits, MediaRef};
pub use transfer::FileOffer;
pub use client::{connect, Client, ConnectOptions, Error, Event, LoginInfo, Security, Session};
pub use messaging::{
    AckKind, ChatUser, IncomingMessage, Presence, PresenceUpdate, Profile, RosterEntry, RosterState,
};
