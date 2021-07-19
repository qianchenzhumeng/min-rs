//! # MIN
//! [MIN](https://github.com/min-protocol/min/wiki) is a simple point-to-point frame based protocol designed to connect a microcontroller to a PC (or other microcontroller).

//! ## Example
//! ```
//! extern crate min;
//! use std::cell::RefCell;
//! 
//! struct App {}
//! 
//! impl App {
//!     fn print_msg(&self, buffer: &[u8], len: u8) {
//!         print!("The data received: [ ");
//!         for i in 0..len {
//!             print!("0x{:02x} ", buffer[i as usize]);
//!         }
//!         println!("]");
//!     }
//! }
//! 
//! struct Uart {
//!     tx_space_avaliable: u16,
//!     rx_buf: RefCell<[u8; 255]>,
//!     rx_buf_index: RefCell<u8>,
//!     loopback: bool,
//! }
//! 
//! impl Uart {
//!     fn open(&self) {
//!         println!("Open uart.");
//!     }
//! 
//!     fn close(&self) {
//!         println!("Close uart.");
//!     }
//! 
//!     fn available_for_write(&self) -> u16 {
//!         self.tx_space_avaliable
//!     }
//! 
//!     fn tx(&self, byte: u8) {
//!         print!("0x{:02x} ", byte);
//!         if self.loopback {
//!             let mut rx_buf = self.rx_buf.borrow_mut();
//!             let mut rx_buf_index = self.rx_buf_index.borrow_mut();
//!             rx_buf[*rx_buf_index as usize] = byte;
//!             *rx_buf_index += 1;
//!         }
//!     }
//! 
//!     fn get_rx_data_len(&self) -> u8 {
//!         *self.rx_buf_index.borrow()
//!     }
//! }
//! 
//! fn tx_start(_: &Uart) {
//!     print!("[ ");
//! }
//! 
//! fn tx_finished(_: &Uart) {
//!     println!("]");
//! }
//! fn tx_space(uart: &Uart) -> u16 {
//!     uart.available_for_write()
//! }
//! 
//! fn tx_byte(uart: &Uart, _port: u8, byte: u8) {
//!     uart.tx(byte);
//! }
//! 
//! fn application_handler(app: &App, _min_id: u8, buffer: &[u8], len: u8, _port: u8) {
//!     app.print_msg(buffer, len);
//! }
//! 
//! fn rx_byte(min: &mut min::Context<Uart, App>, buf: &[u8], buf_len: u32) {
//!     min.poll(buf, buf_len);
//! }
//! 
//! fn main() {
//!     let id: u8 = 0;
//!     let tx_data: [u8; 8] = [0xaa, 0xaa, 0xaa, 0, 0, 0, 0, 1];
//!     let rx_data: [u8; 255] = [0; 255];
//!     let app = App{};
//!     let uart = Uart{
//!         tx_space_avaliable: 128,
//!         rx_buf: RefCell::new(rx_data),
//!         rx_buf_index: RefCell::new(0),
//!         loopback: true,
//!     };
//!     let mut min = min::Context::new(
//!         &uart,
//!         &app,
//!         0,
//!         tx_start,
//!         tx_finished,
//!         tx_space,
//!         tx_byte,
//!         application_handler,
//!     );
//! 
//!     uart.open();
//! 
//!     print!("The data to be sent: [");
//!     for item in tx_data {
//!         print!(" 0x{:x}", item);
//!     }
//!     println!(" ]");
//! 
//!     print!("MIN Frame: ");
//!     let sent = match min.send_frame(id, &tx_data, tx_data.len() as u8) {
//!         Ok(size) => size,
//!         Err(_) => panic!("Opps!"),
//!     };
//! 
//!     assert_eq!(tx_data.len(), sent as usize);
//! 
//!     rx_byte(&mut min, &uart.rx_buf.borrow()[0..255], uart.get_rx_data_len() as u32);
//! 
//!     if 0 != min.get_rx_frame_len() {
//!         println!("The checksum in frame: 0x{:x}", min.get_rx_frame_checksum());
//!     }
//! 
//!     uart.close();
//! }
//! ```

/// Error
pub enum Error {
    /// There is no enough space in tx buffer. The value is the size of bytes overflowed.
    NoEnoughTxSpace(u16),
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

const CRC_REVERSED: bool = true;
const CRC_REFIN: bool = false;
const CRC_REFOUT: bool = false;
const CRC_SEED: u32 = 0xffffffff;
const CRC_POLYNOMIAL_NORMAL: u32 = 0x04c11db7;
const CRC_POLYNOMIAL_REVERSED: u32 = 0xedb88320;

struct Crc32Context {
    crc: u32,
    // Reversed or Normal
    reversed: bool,
    refin: bool,
    refout: bool,
}

impl Crc32Context {
    fn new(reversed: bool, refin: bool, refout: bool) -> Self {
        Crc32Context {
            crc: CRC_SEED,
            reversed: reversed,
            refin: refin,
            refout: refout,
        }
    }

    fn step_normal(&mut self, byte: u8) {
        if self.refin {
            self.crc ^= (byte.reverse_bits() as u32) << 24;
        } else {
            self.crc ^= (byte as u32) << 24;
        }

        for _ in 0..8 {
            if self.crc & 0x80000000 != 0 {
                self.crc = (self.crc << 1) ^ CRC_POLYNOMIAL_NORMAL;
            } else {
                self.crc <<= 1;
            }
        }
    }

    fn step_reversed(&mut self, byte: u8) {
        self.crc ^= byte as u32;
        for _ in 0..8 {
            if self.crc & 1 == 1 {
                self.crc = (self.crc >> 1) ^ CRC_POLYNOMIAL_REVERSED;
            } else {
                self.crc >>= 1;
            }
        }
    }

    fn step(&mut self, byte: u8) {
        if self.reversed {
            self.step_reversed(byte);
        } else {
            self.step_normal(byte);
        }
    }

    fn finalize(&self) -> u32 {
        let crc: u32;

        if self.refout {
            crc = self.crc.reverse_bits();
        } else {
            crc = self.crc;
        }
        !crc
    }
}

/// context for MIN.
pub struct Context<'a, 'b, T, U> {
    /// Hardwar interface
    pub hw_if: &'a T,
    /// Application
    pub app: &'b U,
    /// Number of the port associated with the context
    port: u8,
    /// CALLBACK. Indcates when frame transmission is starting.
    tx_start: fn(hw_if: &'a T),
    /// CALLBACK. Indcates when frame transmission is finished.
    tx_finished: fn(hw_if: &'a T),
    /// CALLBACK. Must return current buffer space.
    /// Used to check that a frame can be queued.
    tx_space: fn(hw_if: &'a T) -> u16,
    /// CALLBACK. Send a byte on the given line.
    tx_byte: fn(hw_if: &'a T, port: u8, byte: u8),
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
    /// CALLBACK. Handle incoming MIN frame
    application_handler: fn(app: &'b U, min_id: u8, buffer: &[u8], len: u8, port: u8),
}

impl<'a, 'b, T, U> Context<'a, 'b, T, U> {
    /// Construct a `Context` for MIN.
    /// # Arguments
    /// * `hw_if` - Reference of hardware interface.
    /// * `app` - Reference of application.
    /// * `port` - Number of the port associated with the context.
    /// * `tx_start` - Callback. Indcates when frame transmission is starting.
    /// * `tx_finished` - Callback. Indcates when frame transmission is finished.
    /// * `tx_space` - Callback. Returns current buffer space.
    /// * `tx_byte` - Callback. Sends a byte on the given line.
    /// * `application_handler` - Callback. Handle incoming MIN frame.
    pub fn new(
        hw_if: &'a T,
        app: &'b U,
        port: u8,
        tx_start: fn(hw_if: &'a T),
        tx_finished: fn(hw_if: &'a T),
        tx_space: fn(hw_if: &'a T) -> u16,
        tx_byte: fn(hw_if: &'a T, port: u8, byte: u8),
        application_handler: fn(app: &'b U, min_id: u8, buffer: &[u8], len: u8, port: u8),
    ) -> Self {
        Context {
            hw_if: hw_if,
            app: app,
            port: port,
            tx_start: tx_start,
            tx_finished: tx_finished,
            tx_space: tx_space,
            tx_byte: tx_byte,
            tx_header_byte_countdown: 2,
            tx_checksum: Crc32Context {
                crc: CRC_SEED,
                reversed: CRC_REVERSED,
                refin: CRC_REFIN,
                refout: CRC_REFOUT,
            },
            rx_header_bytes_seen: 0,
            rx_frame_state: RxState::SearchingForSof,
            rx_frame_id_control: 0,
            rx_frame_payload_bytes: 0,
            rx_checksum: Crc32Context {
                crc: CRC_SEED,
                reversed: CRC_REVERSED,
                refin: CRC_REFIN,
                refout: CRC_REFOUT,
            },
            rx_frame_seq: 0,
            rx_frame_length: 0,
            rx_control: 0,
            rx_frame_payload_buf: [0; MAX_PAYLOAD as usize],
            rx_frame_checksum: 0,
            application_handler: application_handler,
        }
    }

    /// Sends an application MIN frame on the wire (do not put into the transport queue),
    /// returning the number of bytes sent or crate::Error.
    /// # Arguments
    /// * `id` - Identifier/Control
    /// * `payload` - data to send
    /// * `len` - length of payload
    pub fn send_frame(&mut self, id: u8, payload: &[u8], len: u8) -> Result<u8, Error> {
        let avaliable_for_send = (self.tx_space)(&self.hw_if);
        if u16::from(len) <= avaliable_for_send {
            self.on_wire_bytes(id & 0x3f_u8, 0, payload, 0, 0xffff, len);
            Ok(len)
        } else {
            Err(Error::NoEnoughTxSpace((len as u16) - avaliable_for_send))
        }
    }

    fn stuffed_tx_byte(&mut self, byte: u8) {
        // Transmit the byte
        (self.tx_byte)(self.hw_if, self.port, byte);

        self.tx_checksum.step(byte);

        if byte == HEADER_BYTE {
            self.tx_header_byte_countdown -= 1;
            if self.tx_header_byte_countdown == 0 {
                (self.tx_byte)(self.hw_if, self.port, STUFF_BYTE);
                self.tx_header_byte_countdown = 2;
            }
        } else {
            self.tx_header_byte_countdown = 2;
        }
    }

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
        self.tx_checksum = Crc32Context::new(CRC_REVERSED, CRC_REFIN, CRC_REFOUT);

        (self.tx_start)(&self.hw_if);

        // Header is 3 bytes; because unstuffed will reset receiver immediately
        (self.tx_byte)(self.hw_if, self.port, HEADER_BYTE);
        (self.tx_byte)(self.hw_if, self.port, HEADER_BYTE);
        (self.tx_byte)(self.hw_if, self.port, HEADER_BYTE);

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
        (self.tx_byte)(self.hw_if, self.port, EOF_BYTE);

        (self.tx_finished)(self.hw_if);
    }

    /// This runs the receiving half of the transport protocol, acknowledging frames received, discarding
    /// duplicates received, and handling RESET requests.
    fn valid_frame_received(&self) {
        (self.application_handler)(
            self.app,
            self.rx_frame_id_control & 0x3f,
            &self.rx_frame_payload_buf,
            self.rx_control,
            self.port,
        );
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
                self.rx_checksum = Crc32Context::new(CRC_REVERSED, CRC_REFIN, CRC_REFOUT);
                self.rx_checksum.step(byte);
                if byte & 0x80 == 0x80 {
                    // If there is no transport support compiled in then all transport frames are ignored
                    self.rx_frame_state = RxState::SearchingForSof;
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

    /// sends received bytes into a MIN context and runs the transport timeouts.
    pub fn poll(&mut self, buf: &[u8], buf_len: u32) {
        for i in 0..buf_len {
            self.rx_byte(buf[i as usize]);
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
}
