extern crate min_rs as min;
use clap::{crate_authors, crate_name, crate_version, App, Arg};
use env_logger;
use log::{debug, trace, LevelFilter};
use serialport::SerialPort;
use std::cell::RefCell;
use std::io::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct HwIf {
    port: RefCell<Box<dyn SerialPort>>,
    name: String,
    tx_space_avaliable: u16,
    output: Arc<Mutex<String>>,
}

impl HwIf {
    pub fn new(port: Box<dyn SerialPort>, name: String, tx_space_avaliable: u16) -> Self {
        HwIf {
            port: RefCell::new(port),
            name: name,
            tx_space_avaliable: tx_space_avaliable,
            output: Arc::new(Mutex::new(String::from(""))),
        }
    }

    fn available_for_write(&self) -> u16 {
        self.tx_space_avaliable
    }

    fn tx(&self, byte: u8) {
        let mut output = self.output.lock().unwrap();
        output.push_str(format!("0x{:02x} ", byte).as_str());
        let mut port = self.port.borrow_mut();
        match port.write(&[byte]) {
            Ok(_) => {}
            Err(e) => {
                debug!(target: self.name.as_str(), "{}", e);
            }
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, ()> {
        let mut port = self.port.borrow_mut();
        match port.read(&mut buf[..]) {
            Ok(n) => Ok(n),
            _ => Err(()),
        }
    }
}

impl min::Interface for HwIf {
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

    let matches = App::new(crate_name!())
        .version(crate_version!())
        //.long_version(build::version().as_str())
        .author(crate_authors!())
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true)
                .required(false)
                .help("指定端口。"),
        )
        .arg(
            Arg::with_name("baudrate")
                .short("b")
                .long("baudrate")
                .takes_value(true)
                .required(false)
                .help("指定波特率。"),
        )
        .get_matches();
    let baudrate = if let Some(baudrate_str) = matches.value_of("baudrate") {
        if let Ok(b) = baudrate_str.parse::<u32>() {
            b
        } else {
            panic!("波特率非法（{}）！", baudrate_str);
        }
    } else {
        panic!("未指定波特率）！");
    };

    let port = if let Some(port_name) = matches.value_of("port") {
        if let Ok(port) = serialport::new(port_name, baudrate)
            .timeout(Duration::from_millis(10))
            .open()
        {
            port
        } else {
            panic!("端口不存在！");
        }
    } else {
        panic!("未指定端口）！");
    };

    let uart = HwIf::new(port, String::from("uart"), 512);
    let mut min = min::Context::new(String::from("min"), uart, 0, true);

    let mut buf: Vec<u8> = (0..255).collect();
    min.reset_transport(true).unwrap_or(());
    let tx_data: [u8; 3] = [0, 1, 0];
    min.queue_frame(6, &tx_data[..], tx_data.len() as u8)
        .unwrap_or(());
    loop {
        min.poll(&[0][0..0], 0);
        if let Ok(n) = min.hw_if.read(&mut buf[..]) {
            min.poll(&buf[0..n], n as u32);
        };
        if let Ok(msg) = min.get_msg() {
            if let Ok(string) = String::from_utf8(msg.buf[0..msg.len as usize].to_vec()) {
                println!("get msg: {}", string);
            } else {
                println!("get data: {:02X?}", msg.buf);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}
