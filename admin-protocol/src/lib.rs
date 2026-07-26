//! マッチメーカー、AdminServer、GameServer間だけで使う管理通信型。
//!
//! プレイヤーの入力やSnapshotを定義する`pixel-shooter-protocol`とは分離する。

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameServerStatus {
    Available,
    Allocated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationMode {
    Realtime,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameServerRegistration {
    pub server_id: String,
    pub public_url: String,
    pub control_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameServerHeartbeat {
    pub server_id: String,
    pub status: GameServerStatus,
    pub room_id: Option<String>,
    pub player_count: usize,
    pub tick: u64,
    pub simulation_mode: SimulationMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameServerView {
    pub server_id: String,
    pub public_url: String,
    pub control_url: String,
    pub status: GameServerStatus,
    pub room_id: Option<String>,
    pub player_count: usize,
    pub tick: u64,
    pub simulation_mode: SimulationMode,
    pub healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocateRoomRequest {
    pub room_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationResponse {
    pub server_id: String,
    pub room_id: String,
    pub game_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchmakeRequest {
    pub player_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchmakeResponse {
    pub server_id: String,
    pub room_id: String,
    pub game_url: String,
    pub join_ticket: String,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRequest {
    #[serde(default = "default_step_count")]
    pub ticks: u64,
}

fn default_step_count() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlState {
    pub server_id: String,
    pub status: GameServerStatus,
    pub room_id: Option<String>,
    pub player_count: usize,
    pub tick: u64,
    pub simulation_mode: SimulationMode,
    pub pending_steps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinTicketClaims {
    pub room_id: String,
    pub player_name: String,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketError {
    InvalidFormat,
    InvalidPayload,
    InvalidSignature,
    Expired,
}

impl Display for TicketError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidFormat => "invalid ticket format",
            Self::InvalidPayload => "invalid ticket payload",
            Self::InvalidSignature => "invalid ticket signature",
            Self::Expired => "ticket expired",
        };
        formatter.write_str(message)
    }
}

impl Error for TicketError {}

pub fn encode_join_ticket(secret: &[u8], claims: &JoinTicketClaims) -> String {
    let payload = serde_json::to_vec(claims).expect("serializable join ticket claims");
    let payload = URL_SAFE_NO_PAD.encode(payload);
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{payload}.{signature}")
}

pub fn decode_join_ticket(
    secret: &[u8],
    ticket: &str,
    now_unix: u64,
) -> Result<JoinTicketClaims, TicketError> {
    let (payload, signature) = ticket.split_once('.').ok_or(TicketError::InvalidFormat)?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| TicketError::InvalidSignature)?;
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| TicketError::InvalidSignature)?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| TicketError::InvalidPayload)?;
    let claims: JoinTicketClaims =
        serde_json::from_slice(&payload).map_err(|_| TicketError::InvalidPayload)?;
    if claims.expires_at_unix < now_unix {
        return Err(TicketError::Expired);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_ticket_round_trip_and_tamper_detection() {
        let claims = JoinTicketClaims {
            room_id: "room-1".into(),
            player_name: "Player".into(),
            expires_at_unix: 120,
        };
        let ticket = encode_join_ticket(b"secret", &claims);
        let decoded = decode_join_ticket(b"secret", &ticket, 100).expect("valid ticket");
        assert_eq!(decoded.room_id, claims.room_id);
        assert_eq!(decoded.player_name, claims.player_name);

        let tampered = format!("{}x", ticket);
        assert_eq!(
            decode_join_ticket(b"secret", &tampered, 100),
            Err(TicketError::InvalidSignature)
        );
        assert_eq!(
            decode_join_ticket(b"secret", &ticket, 121),
            Err(TicketError::Expired)
        );
    }
}
