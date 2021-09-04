extern crate log;
use crate::crc::Crc32Context;
use crate::transport::*;
use std::time::{SystemTime, UNIX_EPOCH};
use log::{warn, debug, trace};

const CRC_SEED: u32 = 0xffffffff;
const CRC_REVERSED: bool = true;
const CRC_REFIN: bool = false;
const CRC_REFOUT: bool = false;

/// Error
pub enum Error {
    /// There is no enough space in tx buffer. The value is the size of bytes overflowed.
    NoEnoughTxSpace(u16),
    NoMsg,
}

/// Receiving state machine
pub enum RxState {
    SearchingForSof,
    ReceivingIdControl,
    ReceivingSeq,
    ReceivingLength,
    ReceivingPayload,
    ReceivingChecksum3,
    ReceivingChecksum2,
    ReceivingChecksum1,
    ReceivingChecksum0,
    ReceivingEof,
}

// Special protocol bytes
const HEADER_BYTE: u8 = 0xaa;
const STUFF_BYTE: u8 = 0x55;
const EOF_BYTE: u8 = 0x55;

const MAX_PAYLOAD: u8 = u8::MAX;
const MAX_MSG: u8 = 128;

pub struct Msg {
    pub min_id: u8,
    pub len: u8,
    pub buf: Vec<u8>,
    pub port: u8,
}

impl Msg {
    fn new(min_id: u8, payload: &[u8], payload_len: u8, port: u8) ->Self {
        let mut buf: Vec<u8> = Vec::new();
        for i in 0..payload_len {
            buf.push(payload[i as usize]);
        }
        Msg {
            min_id: min_id,
            len: payload_len,
            buf: buf,
            port: port,
        }
    }
}
/// context for MIN.
pub struct Context<'a, T> where T: crate::Interface {
    pub name: String,
    /// Use transport protocol
    pub t_min:  bool,
    /// Hardwar interface
    pub hw_if: &'a T,
    transport: Transport,
    /// Number of the port associated with the context
    port: u8,
    /// Count out the header bytes
    tx_header_byte_countdown: u8,
    /// Calculated checksum for sending frame
    tx_checksum: Crc32Context,
    /// Countdown of header bytes to reset state
    rx_header_bytes_seen: u8,
    /// State of receiver
    rx_frame_state: RxState,
    /// ID and control bit of frame being received
    rx_frame_id_control: u8,
    /// Length of payload received so far
    rx_frame_payload_bytes: u8,
    /// Calculated checksum for receiving frame
    rx_checksum: Crc32Context,
    /// Sequence number of frame being received
    rx_frame_seq: u8,
    /// Length of frame
    rx_frame_length: u8,
    /// Control byte
    rx_control: u8,
    /// Payload received so far
    rx_frame_payload_buf: [u8; MAX_PAYLOAD as usize],
    /// Checksum received over the wire
    rx_frame_checksum: u32,
    msg_queue: Vec<Msg>,
}

impl<'a, T> Context<'a, T> where T: crate::Interface {
    
    fn msg_enqueue(&mut self) {
        let msg = Msg::new(self.rx_frame_id_control & 0x3f, &self.rx_frame_payload_buf, self.rx_control, self.port);
        self.msg_queue.push(msg);
    }

    /// Number of bytes needed for a frame with a given payload length, excluding stuff bytes
    /// 3 header bytes, ID/control byte, length byte, seq byte, 4 byte CRC, EOF byte
    fn on_wire_size(&self, payload_len: u8) -> u16 {
        (payload_len as u16) + 11
    }

    fn stuffed_tx_byte(&mut self, byte: u8) {
        // Transmit the byte
        self.hw_if.tx_byte(self.port, byte);

        self.tx_checksum.step(byte);

        if byte == HEADER_BYTE {
            self.tx_header_byte_countdown -= 1;
            if self.tx_header_byte_countdown == 0 {
                self.hw_if.tx_byte(self.port, STUFF_BYTE);
                self.tx_header_byte_countdown = 2;
            }
        } else {
            self.tx_header_byte_countdown = 2;
        }
    }

    // send min frame on wire.
    fn on_wire_bytes(
        &mut self,
        id_control: u8,
        seq: u8,
        payload_base: &[u8],
        payload_offset: u16,
        payload_mask: u16,
        payload_len: u8,
    ) {
        self.tx_header_byte_countdown = 2;
        self.tx_checksum = Crc32Context::new(CRC_SEED, CRC_REVERSED, CRC_REFIN, CRC_REFOUT);

        self.hw_if.tx_start();

        // Header is 3 bytes; because unstuffed will reset receiver immediately
        self.hw_if.tx_byte(self.port, HEADER_BYTE);
        self.hw_if.tx_byte(self.port, HEADER_BYTE);
        self.hw_if.tx_byte(self.port, HEADER_BYTE);

        self.stuffed_tx_byte(id_control);
        if id_control & 0x80 == 0x80 {
            // Send the sequence number if it is a transport frame
            self.stuffed_tx_byte(seq);
        }

        self.stuffed_tx_byte(payload_len);
        let mut offset: u16 = payload_offset;
        for _ in 0..payload_len {
            self.stuffed_tx_byte(payload_base[offset as usize]);
            offset += 1;
            offset &= payload_mask;
        }

        // send crc checksum
        let checksum = self.tx_checksum.finalize();
        self.stuffed_tx_byte((checksum >> 24) as u8 & 0xff);
        self.stuffed_tx_byte((checksum >> 16) as u8 & 0xff);
        self.stuffed_tx_byte((checksum >> 8) as u8 & 0xff);
        self.stuffed_tx_byte(checksum as u8 & 0xff);

        // Ensure end-of-frame doesn't contain 0xaa and confuse search for start-of-frame
        self.hw_if.tx_byte(self.port, EOF_BYTE);

        self.hw_if.tx_finished();
    }

    // send transport protocol frame on wire.
    fn on_wire_t_frame(&mut self, id: u8, seq: u8, payload: &[u8], len: u8) -> Result<u8, Error> {
        let avaliable_for_send = self.hw_if.tx_space();
        if self.on_wire_size(len) <= avaliable_for_send {
            trace!(target: format!("{}", self.name).as_str(), "on_wire_t_frame: min_id={}, seq={}, payload_len={}", id, seq, len);
            self.on_wire_bytes(id | 0x80_u8, seq, payload, 0, 0xffff, len);
            Ok(len)
        } else {
            warn!(target: format!("{}", self.name).as_str(), "no enough tx space: oversize={}", (len as u16) - avaliable_for_send);
            Err(Error::NoEnoughTxSpace((len as u16) - avaliable_for_send))
        }
    }

    fn transport_fifo_frame_send(&mut self, idx: usize, update_seq: bool) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();
        self.transport.last_received_anything_ms = now;
        // 这个地方需要发送找到的 frame，并且修改该 frame 的最后发送时间。由于借用规则的限制，需要分两步完成。
        if let Some(mut frame) = self.transport.frames.get_mut(idx) {
            frame.last_sent_time_ms = now;
            if update_seq {
                frame.seq = self.transport.sn_max;
            }
        }
        // 这个地方有点疑惑，为什么必须是 `&mut frame`，去掉 `&mut` 会因两次可变借用而编译失败，进一步改为 `get` 后，会因可变借用和不可变借用同时发生而编译失败
        if let Some(&mut frame) = self.transport.frames.get_mut(idx) {
            debug!(target: format!("{}", self.name).as_str(), "send T-Frame: id={}, seq={}, len={}", frame.min_id, frame.seq, frame.payload_len);
            self.on_wire_t_frame(frame.min_id, frame.seq, &frame.payload[0..frame.payload_len as usize], frame.payload_len).unwrap_or(0);
        }
    }

    /// This runs the receiving half of the transport protocol, acknowledging frames received, discarding
    /// duplicates received, and handling RESET requests.
    fn valid_frame_received(&mut self) {
        if self.t_min {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();
            self.transport.last_received_anything_ms = now;
            match self.rx_frame_id_control {
                ACK => {
                    // If we get an ACK then we remove all the acknowledged frames with seq < rn
                    // The payload byte specifies the number of NACKed frames: how many we want retransmitted because
                    // they have gone missing.
                    // But we need to make sure we don't accidentally ACK too many because of a stale ACK from an old session
                    let num_acked = self.rx_frame_seq.wrapping_sub(self.transport.sn_min);
                    let num_nacked = self.rx_frame_payload_buf[0].wrapping_sub(self.rx_frame_seq);  // 好像一直会是 0
                    let num_in_window = self.transport.sn_max.wrapping_sub(self.transport.sn_min);
                    if num_acked <= num_in_window {
                        self.transport.sn_min = self.rx_frame_seq;
                        // Now pop off all the frames up to (but not including) rn
                        // The ACK contains Rn; all frames before Rn are ACKed and can be removed from the window
                        debug!(target: format!("{}", self.name).as_str(), "Received ACK seq={}, num_acked={}, num_nacked={}", self.rx_frame_seq, num_acked, num_nacked);
                        for _ in 0..num_acked {
                            debug!(target: format!("{}", self.name).as_str(), "Pop transport fifo.");
                            self.transport.pop();
                        }
                        // Now retransmit the number of frames that were requested
                        for i in 0..num_nacked {
                            self.transport_fifo_frame_send(i.into(), false);
                        }
                    } else {
                        debug!(target: format!("{}", self.name).as_str(), "Received spurious ACK seq={}", self.rx_frame_seq);
                        self.transport.spurious_acks = self.transport.spurious_acks.wrapping_add(1);
                    }
                },
                RESET => {
                    // If we get a RESET demand then we reset the transport protocol (empty the FIFO, reset the
                    // sequence numbers, etc.)
                    // We don't send anything, we just do it. The other end can send frames to see if this end is
                    // alive (pings, etc.) or just wait to get application frames.
                    self.transport.resets_received = self.transport.resets_received.wrapping_add(1);
                    self.transport.reset_transport_fifo();
                },
                _ => {
                    if self.rx_frame_id_control & 0x80 == 0x80 {
                        // Incoming application frames
                        // Reset the activity time (an idle connection will be stalled)
                        self.transport.last_received_frame_ms = now;
                        if self.rx_frame_seq == self.transport.rn {
                            debug!(target: format!("{}", self.name).as_str(), "Incoming T-MIN frame seq={}, id={}, payload len={}",
                                self.rx_frame_seq, self.rx_frame_id_control & 0x3f, self.rx_control);
                            // Now looking for the next one in the sequence
                            self.transport.rn = self.transport.rn.wrapping_add(1);
                            // Always send an ACK back for the frame we received
                            // ACKs are short (should be about 9 microseconds to send on the wire) and
                            // this will cut the latency down.
                            // We also periodically send an ACK in case the ACK was lost, and in any case
                            // frames are re-sent.
                            self.send_ack();
                            // Now ready to pass this up to the application handlers

                            self.msg_enqueue();
                        } else {
                            // Discard this frame because we aren't looking for it: it's either a dupe because it was
                            // retransmitted when our ACK didn't get through in time, or else it's further on in the
                            // sequence and others got dropped.
                            warn!(target: format!("{}", self.name).as_str(), "sequence mismatch: seq={}, rn={}", self.rx_frame_seq, self.transport.rn);
                            self.transport.sequence_mismatch_drop = self.transport.sequence_mismatch_drop.wrapping_add(1);
                        }
                    } else {
                        debug!(target: format!("{}", self.name).as_str(), "Incoming MIN frame id={}, payload len={}", self.rx_frame_id_control & 0x3f, self.rx_control);
                        // Not a transport frame
                        self.msg_enqueue();
                    }
                },
            }
        } else {
            debug!(target: format!("{}", self.name).as_str(), "Incoming app frame id={}, payload len={}",
                self.rx_frame_id_control & 0x3f, self.rx_control);
                self.msg_enqueue();
        }
    }

    fn rx_byte(&mut self, byte: u8) {
        // Regardless of state, three header bytes means "start of frame" and
        // should reset the frame buffer and be ready to receive frame data.
        //
        // Two in a row in over the frame means to expect a stuff byte.
        if self.rx_header_bytes_seen == 2 {
            self.rx_header_bytes_seen = 0;
            match byte {
                HEADER_BYTE => {
                    self.rx_frame_state = RxState::ReceivingIdControl;
                    return;
                }
                STUFF_BYTE => {
                    // Discard this byte; carry on receiving on the next character.
                    return;
                }
                _ => {
                    // Something has gone wrong, give up on this frame and look for header again.
                    self.rx_frame_state = RxState::SearchingForSof;
                }
            }
        }

        if byte == HEADER_BYTE {
            self.rx_header_bytes_seen += 1;
        } else {
            self.rx_header_bytes_seen = 0;
        }

        match self.rx_frame_state {
            RxState::SearchingForSof => {}
            RxState::ReceivingIdControl => {
                self.rx_frame_id_control = byte;
                self.rx_frame_payload_bytes = 0;
                self.rx_checksum = Crc32Context::new(CRC_SEED, CRC_REVERSED, CRC_REFIN, CRC_REFOUT);
                self.rx_checksum.step(byte);
                if byte & 0x80 == 0x80 {
                    if self.t_min {
                        self.rx_frame_state = RxState::ReceivingSeq;
                    } else {
                        // If there is no transport support compiled in then all transport frames are ignored
                        warn!(target: format!("{}", self.name).as_str(), "no transport support, drop this frame.");
                        self.rx_frame_state = RxState::SearchingForSof;
                    }
                } else {
                    self.rx_frame_seq = 0;
                    self.rx_frame_state = RxState::ReceivingLength;
                }
            }
            RxState::ReceivingSeq => {
                self.rx_frame_seq = byte;
                self.rx_checksum.step(byte);
                self.rx_frame_state = RxState::ReceivingLength;
            }
            RxState::ReceivingLength => {
                self.rx_frame_length = byte;
                self.rx_control = byte;
                self.rx_checksum.step(byte);
                if self.rx_frame_length > 0 {
                    if self.rx_frame_length <= MAX_PAYLOAD {
                        self.rx_frame_state = RxState::ReceivingPayload;
                    } else {
                        // Frame dropped because it's longer than any frame we can buffer
                        self.rx_frame_state = RxState::SearchingForSof;
                    }
                } else {
                    self.rx_frame_state = RxState::ReceivingChecksum3;
                }
            }
            RxState::ReceivingPayload => {
                self.rx_frame_payload_buf[self.rx_frame_payload_bytes as usize] = byte;
                self.rx_frame_payload_bytes += 1;
                self.rx_checksum.step(byte);
                self.rx_frame_length -= 1;
                if self.rx_frame_length == 0 {
                    self.rx_frame_state = RxState::ReceivingChecksum3;
                }
            }
            RxState::ReceivingChecksum3 => {
                self.rx_frame_checksum = (byte as u32) << 24;
                self.rx_frame_state = RxState::ReceivingChecksum2;
            }
            RxState::ReceivingChecksum2 => {
                self.rx_frame_checksum |= (byte as u32) << 16;
                self.rx_frame_state = RxState::ReceivingChecksum1;
            }
            RxState::ReceivingChecksum1 => {
                self.rx_frame_checksum |= (byte as u32) << 8;
                self.rx_frame_state = RxState::ReceivingChecksum0;
            }
            RxState::ReceivingChecksum0 => {
                self.rx_frame_checksum |= byte as u32;
                let crc = self.rx_checksum.finalize();
                if crc != self.rx_frame_checksum {
                    // Frame fails the checksum and so is dropped
                    warn!(target: format!("{}", self.name).as_str(), "crc error, drop this frame.");
                    self.rx_frame_state = RxState::SearchingForSof;
                } else {
                    // Checksum passes, go on to check for the end-of-frame marker
                    self.rx_frame_state = RxState::ReceivingEof;
                }
            }
            RxState::ReceivingEof => {
                if byte == EOF_BYTE {
                    // Frame received OK, pass up data to handler
                    self.valid_frame_received();
                }
                // else discard
                // Look for next frame */
                self.rx_frame_state = RxState::SearchingForSof;
            }
        }
    }

    fn find_retransmit_frame(&mut self) -> (usize, u128) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();
        let window_size = self.transport.sn_max.wrapping_sub(self.transport.sn_min);
        let mut oldest_elapsed_time: u128 = 0;
        let mut oldest_frame_index: usize = 0;
        let mut last_sent_time_ms = 0;
        for i in 0..window_size {
            if let Some(frame) = self.transport.frames.get(i.into()) {
                let elapsed = now.wrapping_sub(frame.last_sent_time_ms);
                if elapsed > oldest_elapsed_time {
                    oldest_elapsed_time = elapsed;
                    oldest_frame_index = i.into();
                    last_sent_time_ms = frame.last_sent_time_ms;
                }
            }
        }
        (oldest_frame_index, last_sent_time_ms)
    }

    fn push(&mut self, frame: TransportFrame) {
        self.transport.frames.push_back(frame);
        self.transport.n_frames = self.transport.n_frames.wrapping_add(1);
        if self.transport.n_frames_max < self.transport.n_frames {
            self.transport.n_frames_max = self.transport.n_frames;
        }
        debug!(target: format!("{}", self.name).as_str(), "Queued ID={}, len={}", frame.min_id, frame.payload_len);
    }

    fn send_ack(&mut self) {
        let now =SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();
        debug!(target: format!("{}", self.name).as_str(), "send ACK: seq={}", self.transport.rn);
        self.on_wire_t_frame(ACK, self.transport.rn, &[self.transport.rn][0..1], 1).unwrap_or(0);
        self.transport.last_sent_ack_time_ms = now;
    }

    fn send_reset(&mut self) {
        debug!(target: format!("{}", self.name).as_str(), "send RESET");
        self.on_wire_bytes(RESET, 0, &[0][0..0], 0, 0, 0);
    }
}

impl<'a, T> Context<'a, T> where T: crate::Interface{
    /// Construct a `Context` for MIN.
    /// # Arguments
    /// * `name` - identifier string for debug.
    /// * `hw_if` - Reference of hardware interface.
    /// * `port` - Number of the port associated with the context.
    /// * `t_min` - Use transport protocol.
    /// * `tx_start` - Callback. Indcates when frame transmission is starting.
    /// * `tx_finished` - Callback. Indcates when frame transmission is finished.
    /// * `tx_space` - Callback. Returns current buffer space.
    /// * `tx_byte` - Callback. Sends a byte on the given line.
    pub fn new(
        name: String,
        hw_if: &'a T,
        port: u8,
        t_min: bool,
    ) -> Self {
        Context {
            transport: Transport::new(),
            hw_if: hw_if,
            name: name,
            port: port,
            t_min: t_min,
            tx_header_byte_countdown: 2,
            tx_checksum: Crc32Context::new(CRC_SEED, CRC_REVERSED, CRC_REFIN, CRC_REFOUT),
            rx_header_bytes_seen: 0,
            rx_frame_state: RxState::SearchingForSof,
            rx_frame_id_control: 0,
            rx_frame_payload_bytes: 0,
            rx_checksum: Crc32Context::new(CRC_SEED, CRC_REVERSED, CRC_REFIN, CRC_REFOUT),
            rx_frame_seq: 0,
            rx_frame_length: 0,
            rx_control: 0,
            rx_frame_payload_buf: [0; MAX_PAYLOAD as usize],
            rx_frame_checksum: 0,
            msg_queue: Vec::with_capacity(MAX_MSG as usize),
        }
    }

    /// Sends an application MIN frame on the wire (do not put into the transport queue),
    /// returning the number of bytes sent or crate::Error.
    /// # Arguments
    /// * `id` - Identifier/Control
    /// * `payload` - data to send
    /// * `len` - length of payload
    pub fn send_frame(&mut self, id: u8, payload: &[u8], len: u8) -> Result<u8, Error> {
        let avaliable_for_send = self.hw_if.tx_space();
        if self.on_wire_size(len) <= avaliable_for_send {
            self.on_wire_bytes(id & 0x3f_u8, 0, payload, 0, 0xffff, len);
            Ok(len)
        } else {
            Err(Error::NoEnoughTxSpace((len as u16) - avaliable_for_send))
        }
    }

    pub fn reset_transport(&mut self, inform_other_side: bool) -> Result<(), String> {
        if self.t_min {
            debug!(target: format!("{}", self.name).as_str(), "reset transport(clear the fifo, restart timing).");
            if inform_other_side {
                self.send_reset();
            }
            self.transport.reset_transport_fifo();
            Ok(())
        } else {
            warn!(target: format!("{}", self.name).as_str(), "no transport support.");
            Err(String::from("no transport support."))
        }
    }

    /// Queues a MIN ID / payload frame into the outgoing FIFO(T-MIN only)
    /// Returns true if the frame was queued or false if context doesn't support transport protocol
    pub fn queue_frame(&mut self, id: u8, payload: &[u8], len: u8) -> Result<(), String> {
        if self.t_min {
            let frame = TransportFrame::new(id, payload, len);
            self.push(frame);
            Ok(())
        } else {
            warn!(target: format!("{}", self.name).as_str(), "no transport support.");
            Err(String::from("no transport support."))
        }
    }

    /// sends received bytes into a MIN context and runs the transport timeouts.
    pub fn poll(&mut self, buf: &[u8], buf_len: u32) {
        for i in 0..buf_len {
            self.rx_byte(buf[i as usize]);
        }

        // for T-MIN
        if self.t_min {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();
            let mut remote_connected = false;
            let mut remote_active = false;
            if now.wrapping_sub(self.transport.last_received_anything_ms) < TRANSPORT_IDLE_TIMEOUT_MS {
                remote_connected = true;
            }
            if now.wrapping_sub(self.transport.last_received_frame_ms) < TRANSPORT_IDLE_TIMEOUT_MS {
                remote_active = true;
            }
            let window_size = self.transport.sn_max.wrapping_sub(self.transport.sn_min);
            if (window_size < TRANSPORT_MAX_WINDOW_SIZE) && (self.transport.n_frames > window_size) {
                debug!(target: format!("{}", self.name).as_str(), "Send new frames(window_size={}, sn_max={}, sn_min={}, n_frames={})",
                    window_size, self.transport.sn_max, self.transport.sn_min, self.transport.n_frames
                );
                // There are new frames we can send; but don't even bother if there's no buffer space for them
                self.transport_fifo_frame_send(window_size as usize, true);
                self.transport.sn_max = self.transport.sn_max.wrapping_add(1);
            } else {
                // Sender cannot send new frames so resend old ones (if there's anyone there)
                if (window_size > 0) && remote_connected {
                    // There are unacknowledged frames. Can re-send an old frame. Pick the least recently sent one.
                    let (index, last_sent_time_ms) = self.find_retransmit_frame();
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_millis();
                    if now.wrapping_sub(last_sent_time_ms) >= TRANSPORT_FRAME_RETRANSMIT_TIMEOUT_MS {
                        debug!(target: format!("{}", self.name).as_str(), "Send old frames(window_size={}, sn_max={}, sn_min={}, n_frames={})",
                            window_size, self.transport.sn_max, self.transport.sn_min, self.transport.n_frames
                        );
                        self.transport_fifo_frame_send(index, false);
                    }
                }
            }
    
            // 发送 ack
            if now.wrapping_sub(self.transport.last_sent_ack_time_ms) > TRANSPORT_ACK_RETRANSMIT_TIMEOUT_MS {
                if remote_active {
                    self.send_ack();
                }
            }
        }
    }

    pub fn get_msg(&mut self) -> Result<Msg, Error> {
        match self.msg_queue.pop() {
            Some(msg) => {
                Ok(msg)
            },
            None => {
                Err(Error::NoMsg)
            }
        }
    }
    pub fn get_rx_checksum(&self) -> u32 {
        self.rx_checksum.finalize()
    }

    pub fn get_rx_frame_checksum(&self) -> u32 {
        self.rx_frame_checksum
    }

    pub fn get_rx_frame_len(&self) -> u8 {
        self.rx_control
    }

    pub fn get_reset_cnt(&self) -> u32 {
        self.transport.get_reset_cnt()
    }

    pub fn get_spurious_ack_cnt(&self) -> u32 {
        self.transport.get_spurious_ack_cnt()
    }

    pub fn get_drop_cnt(&self) -> u32 {
        self.transport.get_drop_cnt()
    }
}
