//! # MIN
//! [MIN](https://github.com/min-protocol/min/wiki) is a simple point-to-point frame based protocol designed to connect a microcontroller to a PC (or other microcontroller).

//! ## Example
//! ```
//! extern crate min_rs as min;
//! use std::thread;
//! use std::sync::mpsc::{Sender, Receiver, channel};
//! 
//! struct Uart {
//!     name: String,
//!     tx_space_avaliable: u16,
//!     sender: Sender<u8>,
//!     receiver: Receiver<u8>,
//! }
//! 
//! impl Uart {
//!     fn new(name: String, tx_space_avaliable: u16, sender: Sender<u8>, receiver: Receiver<u8>) -> Self {
//!         Uart{
//!             name: name,
//!             tx_space_avaliable: tx_space_avaliable,
//!             sender: sender,
//!             receiver: receiver,
//!         }
//!     }
//! 
//!     fn open(&self) {
//!         println!("{}: Open uart.", self.name);
//!     }
//! 
//!     fn close(&self) {
//!         println!("{}: Close uart.", self.name);
//!     }
//! 
//!     fn available_for_write(&self) -> u16 {
//!         self.tx_space_avaliable
//!     }
//! 
//!     fn tx(&self, byte: u8) {
//!         print!("0x{:02x} ", byte);
//!         match self.sender.send(byte) {
//!             Ok(_) => {},
//!             Err(e) => {
//!                 println!("{}: {}", self.name, e);
//!             },
//!         }
//!     }
//! }
//! 
//! fn tx_start(uart: &Uart) {
//!     print!("{} send frame: [ ", uart.name);
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
//! 
//! fn rx_byte(min: &mut min::Context<Uart>, buf: &[u8], buf_len: u32) {
//!     min.poll(buf, buf_len);
//! }
//! 
//! fn main() {
//!     let id: u8 = 0;
//!     let tx_data: [u8; 8] = [0xaa, 0xaa, 0xaa, 0, 0, 0, 0, 1];
//!     let (tx1, rx2) = channel();
//!     let (tx2, rx1) = channel();
//!     
//!     let app1_builder = thread::Builder::new().name("app1".into());
//!     let app1 = app1_builder.spawn(move || {
//!         let uart1 = Uart::new(String::from("uart1"), 128, tx1, rx1);
//!         let mut min1 = min::Context::new(
//!             String::from("min1"),
//!             &uart1,
//!             0,
//!             false,
//!             tx_start,
//!             tx_finished,
//!             tx_space,
//!             tx_byte,
//!         );
//!         min1.hw_if.open();
//! 
//!         print!("The data to be sent: [");
//!         for item in tx_data {
//!             print!(" 0x{:x}", item);
//!         }
//!         println!(" ]");
//! 
//!         match min1.send_frame(id, &tx_data, tx_data.len() as u8) {
//!             Ok(size) => {
//!                 println!("{}: {} bytes sent.", min1.hw_if.name, size);
//!             },
//!             Err(_) => panic!("Opps!"),
//!         };
//! 
//!         min1.hw_if.close();
//!     }).unwrap();
//!     
//!     let app2_builder = thread::Builder::new().name("app2".into());
//!     let app2 = app2_builder.spawn(move || {
//!         let uart2 = Uart::new(String::from("uart2"), 128, tx2, rx2);
//!         let mut min2 = min::Context::new(
//!             String::from("min2"),
//!             &uart2,
//!             0,
//!             false,
//!             tx_start,
//!             tx_finished,
//!             tx_space,
//!             tx_byte,
//!         );
//!         min2.hw_if.open();
//! 
//!         for byte in min2.hw_if.receiver.iter() {
//!             rx_byte(&mut min2, &[byte as u8][0..1], 1);
//!         }
//! 
//!         match min2.get_msg() {
//!             Ok(msg) => {
//!                 print!("app2 receive data: [ ");
//!                 for i in 0..msg.len {
//!                     print!("0x{:02x} ", msg.buf[i as usize]);
//!                 }
//!                 println!("]");
//!             },
//!             Err(_) => {
//!                 println!("No msg!");
//!             }
//!     }
//!         min2.hw_if.close();
//!     }).unwrap();
//! 
//!     app1.join().unwrap();
//!     app2.join().unwrap();
//! }
//! ```

mod crc;
mod transport;

pub mod context;

pub use context::*;
