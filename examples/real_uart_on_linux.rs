extern crate serial;
extern crate min_rs as min;

use std::time::Duration;
use std::thread;
use serial::prelude::*;
use serial::SystemPort;
use std::io::prelude::*;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use log::{LevelFilter, debug, trace};
use env_logger;

const SERIAL_PORT: &str = "/dev/ttyS5";
const BAUD_RATE: serial::BaudRate = serial::Baud115200;

struct Uart {
    port: RefCell<SystemPort>,
    name: String,
    tx_space_avaliable: u16,
    output: Arc<Mutex<String>>,
}

impl Uart {
    fn new(port: SystemPort, name: String, tx_space_avaliable: u16) -> Self {
        Uart{
            port: RefCell::new(port),
            name: name,
            tx_space_avaliable: tx_space_avaliable,
            output: Arc::new(Mutex::new(String::from(""))),
        }
    }

    fn open(&self) {
        const SETTINGS: serial::PortSettings = serial::PortSettings {
            baud_rate: BAUD_RATE,
            char_size: serial::Bits8,
            parity: serial::ParityNone,
            stop_bits: serial::Stop1,
            flow_control: serial::FlowNone,
        };
        let mut port = self.port.borrow_mut();
        port.configure(&SETTINGS).unwrap();
        port.set_timeout(Duration::from_millis(1000)).unwrap();
        debug!(target: self.name.as_str(), "{}: Open uart.", self.name);
    }

    fn available_for_write(&self) -> u16 {
        self.tx_space_avaliable
    }

    fn tx(&self, byte: u8) {
        let mut output = self.output.lock().unwrap();
        output.push_str(format!("0x{:02x} ", byte).as_str());
        let mut port = self.port.borrow_mut();
        match port.write(&[byte]) {
            Ok(_) => {},
            Err(e) => {
                debug!(target: self.name.as_str(), "{}", e);
            },
        }
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize, ()> {
        let mut port = self.port.borrow_mut();
        match port.read(&mut buf[..]) {
            Ok(n) => Ok(n),
            _ => Err(()),
        }
    }
}

impl min::Interface for Uart {
    fn tx_start(&self) {
        let mut output = self.output.lock().unwrap();
        output.clear();
        output.push_str(format!("send frame: [ ").as_str());
    }
    
    fn tx_finished(&self) {
        let mut output = self.output.lock().unwrap();
        output.push_str(format!("]").as_str());
        trace!(target: self.name.as_str(), "{}", output);
    }
    fn tx_space(&self) -> u16 {
        self.available_for_write()
    }
    
    fn tx_byte(&self, _min_port: u8, byte: u8) {
        self.tx(byte);
    }
}

fn main() {
    log::set_max_level(LevelFilter::Debug);
    env_logger::init();
    let tx_data: [u8; 3] = [1, 2, 3];
    let port = serial::open(SERIAL_PORT).unwrap();
    let uart = Uart::new(port, String::from("uart"), 128);
    let mut min = min::Context::new(
        String::from("min"),
        &uart,
        0,
        true,
    );
    min.hw_if.open();

    let mut buf: Vec<u8> = (0..255).collect();
    min.reset_transport(true).unwrap_or(());
    min.queue_frame(0, &tx_data[..], tx_data.len() as u8).unwrap_or(());
    loop {
        min.poll(&[0][0..0], 0);
        if let Ok(n) = min.hw_if.read(&mut buf[..]) {
            min.poll(&buf[0..n], n as u32);
        };
        if let Ok(msg) = min.get_msg() {
            if let Ok(string) = String::from_utf8(msg.buf[0..msg.len as usize].to_vec()) {
                println!("get msg: {}", string);
            } else {
                print!("get data: [ ");
                for i in 0..msg.len {
                    print!("0x{:02x} ", msg.buf[i as usize]);
                }
                println!("]");
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}