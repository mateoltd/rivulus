//! Gateway opcodes (0,1,2,3,4,6,7,8,9,10,11 — no voice ops).
//!
//! Tolerant parsing: unknown values decode as `Unknown(u8)` instead of
//! erroring, so forward-compatible payloads survive. Manual `u8`
//! mapping only; no `serde(other)` tricks.

/// Opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// Dispatch.
    Dispatch,
    /// Heartbeat.
    Heartbeat,
    /// Identify.
    Identify,
    /// Presence update.
    PresenceUpdate,
    /// Voice state update.
    VoiceStateUpdate,
    /// Resume.
    Resume,
    /// Reconnect (server request).
    Reconnect,
    /// Request guild members.
    RequestGuildMembers,
    /// Invalid session.
    InvalidSession,
    /// Hello.
    Hello,
    /// Heartbeat ACK.
    HeartbeatAck,
    /// Unknown (forward-compatible).
    Unknown(u8),
}

impl Opcode {
    /// Numeric value.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Dispatch => 0,
            Self::Heartbeat => 1,
            Self::Identify => 2,
            Self::PresenceUpdate => 3,
            Self::VoiceStateUpdate => 4,
            Self::Resume => 6,
            Self::Reconnect => 7,
            Self::RequestGuildMembers => 8,
            Self::InvalidSession => 9,
            Self::Hello => 10,
            Self::HeartbeatAck => 11,
            Self::Unknown(n) => n,
        }
    }
    /// Tolerant parse: unknown values become `Unknown(n)`, never fail.
    #[must_use]
    pub fn from_u8(n: u8) -> Self {
        match n {
            0 => Self::Dispatch,
            1 => Self::Heartbeat,
            2 => Self::Identify,
            3 => Self::PresenceUpdate,
            4 => Self::VoiceStateUpdate,
            6 => Self::Resume,
            7 => Self::Reconnect,
            8 => Self::RequestGuildMembers,
            9 => Self::InvalidSession,
            10 => Self::Hello,
            11 => Self::HeartbeatAck,
            other => Self::Unknown(other),
        }
    }
}

impl From<u8> for Opcode {
    fn from(n: u8) -> Self {
        Self::from_u8(n)
    }
}

impl From<Opcode> for u8 {
    fn from(op: Opcode) -> Self {
        op.as_u8()
    }
}

impl serde::Serialize for Opcode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.as_u8())
    }
}

struct OpcodeVisitor;

impl serde::de::Visitor<'_> for OpcodeVisitor {
    type Value = Opcode;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a gateway opcode 0..=11 or unknown u8")
    }
    fn visit_u8<E: serde::de::Error>(self, v: u8) -> Result<Opcode, E> {
        Ok(Opcode::from_u8(v))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Opcode, E> {
        let n = u8::try_from(v).map_err(|_| E::custom("opcode out of range"))?;
        Ok(Opcode::from_u8(n))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Opcode, E> {
        let n = u8::try_from(v).map_err(|_| E::custom("opcode out of range"))?;
        Ok(Opcode::from_u8(n))
    }
    fn visit_u16<E: serde::de::Error>(self, v: u16) -> Result<Opcode, E> {
        let n = u8::try_from(v).map_err(|_| E::custom("opcode out of range"))?;
        Ok(Opcode::from_u8(n))
    }
    fn visit_u32<E: serde::de::Error>(self, v: u32) -> Result<Opcode, E> {
        let n = u8::try_from(v).map_err(|_| E::custom("opcode out of range"))?;
        Ok(Opcode::from_u8(n))
    }
}

impl<'de> serde::Deserialize<'de> for Opcode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_u8(OpcodeVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_known() {
        for (n, op) in [
            (0, Opcode::Dispatch),
            (1, Opcode::Heartbeat),
            (2, Opcode::Identify),
            (3, Opcode::PresenceUpdate),
            (4, Opcode::VoiceStateUpdate),
            (6, Opcode::Resume),
            (7, Opcode::Reconnect),
            (8, Opcode::RequestGuildMembers),
            (9, Opcode::InvalidSession),
            (10, Opcode::Hello),
            (11, Opcode::HeartbeatAck),
        ] {
            assert_eq!(op.as_u8(), n);
            assert_eq!(Opcode::from_u8(n), op);
            let ser = serde_json::to_value(op).expect("ser");
            assert_eq!(ser, serde_json::Value::from(n));
            let de: Opcode = serde_json::from_value(serde_json::Value::from(n)).expect("de");
            assert_eq!(de, op);
        }
    }

    #[test]
    fn unknown_tolerant() {
        for n in [5u8, 12, 42, 255] {
            let op = Opcode::from_u8(n);
            assert_eq!(op, Opcode::Unknown(n));
            assert_eq!(op.as_u8(), n);
            let ser = serde_json::to_value(op).expect("ser");
            assert_eq!(ser, serde_json::Value::from(n));
            let de: Opcode = serde_json::from_value(serde_json::Value::from(n)).expect("de");
            assert_eq!(de, Opcode::Unknown(n));
        }
    }

    #[test]
    fn header_peek_op() {
        let raw = br#"{"op":10,"d":{"heartbeat_interval":41250}}"#;
        let header: common::json::Header<'_> = common::json::from_slice(raw).expect("header");
        assert_eq!(Opcode::from_u8(header.op), Opcode::Hello);
    }
}
