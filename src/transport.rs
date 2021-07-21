use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

pub const TRANSPORT_MAX_PAYLOAD_LEN: u8 = u8::MAX;
pub const TRANSPORT_FIFO_MAX_FRAMES: u8 = 31;
pub const TRANSPORT_MAX_WINDOW_SIZE: u8 = 16;

pub const TRANSPORT_IDLE_TIMEOUT_MS: u128 = 500;
pub const TRANSPORT_ACK_RETRANSMIT_TIMEOUT_MS: u128 = 250;
pub const TRANSPORT_FRAME_RETRANSMIT_TIMEOUT_MS: u128 = 1000;

pub const ACK: u8 = 0xff;
pub const RESET: u8 = 0xfe;

#[derive(Copy, Clone)]
pub struct TransportFrame {
    /// When frame was last sent (used for re-send timeouts)
    pub last_sent_time_ms: u128,
    pub payload: [u8; TRANSPORT_MAX_PAYLOAD_LEN as usize],
    /// How big the payload is
    pub payload_len: u8,
    /// ID of frame
    pub min_id: u8,
    /// Sequence number of frame
    pub seq: u8,
}

impl TransportFrame {
    pub fn new(min_id: u8, payload: &[u8], len: u8) -> Self {
        let mut frame = TransportFrame {
            last_sent_time_ms: 0,
            payload: [0_u8; TRANSPORT_MAX_PAYLOAD_LEN as usize],
            payload_len: len,
            min_id: min_id,
            seq: 0,
        };
        for i in 0..len as usize {
            frame.payload[i] = payload[i];
        }
        frame
    }
}

pub struct Transport {
    pub frames: VecDeque<TransportFrame>,
    pub last_sent_ack_time_ms: u128,
    pub last_received_anything_ms: u128,
    pub last_received_frame_ms: u128,
    pub spurious_acks: u32,
    pub sequence_mismatch_drop: u32,
    pub resets_received: u32,
    /// Number of frames in the FIFO
    pub n_frames: u8,
    /// Larger number of frames in the FIFO
    pub n_frames_max: u8,
    /// Sequence numbers for transport protocol
    pub sn_min: u8,
    pub sn_max: u8,
    pub rn: u8,
}

impl Transport {
    pub fn new() -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();
        Transport{
            frames: VecDeque::with_capacity(TRANSPORT_FIFO_MAX_FRAMES as usize),
            last_sent_ack_time_ms: now,
            last_received_anything_ms: now,
            last_received_frame_ms: 0,
            spurious_acks: 0,
            sequence_mismatch_drop: 0,
            resets_received: 0,
            n_frames: 0,
            n_frames_max: 0,
            sn_min: 0,
            sn_max: 0,
            rn: 0,
        }
    }

    pub fn reset_transport_fifo(&mut self) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();

        // Clear down the transmission FIFO queue
        self.frames.clear();
        self.n_frames = 0;
        self.sn_max = 0;
        self.sn_min = 0;
        self.rn = 0;

        // Reset the timers
        self.last_received_anything_ms = now;
        self.last_sent_ack_time_ms = now;
        self.last_received_frame_ms = 0;
    }

    pub fn pop(&mut self) {
        self.frames.pop_front();
        self.n_frames -= 1;
    }

    pub fn get_drop_cnt(&self) -> u32 {
        self.sequence_mismatch_drop
    }

    pub fn get_reset_cnt(&self) -> u32 {
        self.resets_received
    }

    pub fn get_spurious_ack_cnt(&self) -> u32 {
        self.spurious_acks
    }
}
