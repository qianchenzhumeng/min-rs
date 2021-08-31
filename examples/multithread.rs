extern crate min_rs as min;
use std::thread;
use std::sync::mpsc::{Sender, Receiver, channel};
use log::LevelFilter;
use env_logger;

struct Uart {
    name: String,
    tx_space_avaliable: u16,
    sender: Sender<u8>,
    receiver: Receiver<u8>,
}

impl Uart {
    fn new(name: String, tx_space_avaliable: u16, sender: Sender<u8>, receiver: Receiver<u8>) -> Self {
        Uart{
            name: name,
            tx_space_avaliable: tx_space_avaliable,
            sender: sender,
            receiver: receiver,
        }
    }

    fn open(&self) {
        println!("{}: Open uart.", self.name);
    }

    fn close(&self) {
        println!("{}: Close uart.", self.name);
    }

    fn available_for_write(&self) -> u16 {
        self.tx_space_avaliable
    }

    fn tx(&self, byte: u8) {
        print!("0x{:02x} ", byte);
        match self.sender.send(byte) {
            Ok(_) => {},
            Err(e) => {
                println!("{}: {}", self.name, e);
            },
        }
    }
}

impl min::Interface for Uart {
    fn tx_start(&self) {
        print!("{} send frame: [ ", self.name);
    }
    
    fn tx_finished(&self) {
        println!("]");
    }
    fn tx_space(&self) -> u16 {
        self.available_for_write()
    }
    
    fn tx_byte(&self, _port: u8, byte: u8) {
        self.tx(byte);
    }
}

fn main() {
    log::set_max_level(LevelFilter::Trace);
    env_logger::init();

    let id: u8 = 0;
    let tx_data: [u8; 8] = [0xaa, 0xaa, 0xaa, 0, 0, 0, 0, 1];
    let (tx1, rx2) = channel();
    let (tx2, rx1) = channel();
    
    let app1_builder = thread::Builder::new().name("app1".into());
    let app1 = app1_builder.spawn(move || {
        let uart1 = Uart::new(String::from("uart1"), 128, tx1, rx1);
        let mut min1 = min::Context::new(
            String::from("min1"),
            &uart1,
            0,
            false,
        );
        min1.hw_if.open();

        print!("The data to be sent: [");
        for item in tx_data {
            print!(" 0x{:x}", item);
        }
        println!(" ]");

        match min1.send_frame(id, &tx_data, tx_data.len() as u8) {
            Ok(size) => {
                println!("{}: {} bytes sent.", min1.hw_if.name, size);
            },
            Err(_) => panic!("Opps!"),
        };

        min1.hw_if.close();
    }).unwrap();
    
    let app2_builder = thread::Builder::new().name("app2".into());
    let app2 = app2_builder.spawn(move || {
        let uart2 = Uart::new(String::from("uart2"), 128, tx2, rx2);
        let mut min2 = min::Context::new(
            String::from("min2"),
            &uart2,
            0,
            false,
        );
        min2.hw_if.open();

        for byte in min2.hw_if.receiver.iter() {
            min2.poll(&[byte as u8][0..1], 1);
        }

        match min2.get_msg() {
            Ok(msg) => {
                print!("app2 receive data: [ ");
                for i in 0..msg.len {
                    print!("0x{:02x} ", msg.buf[i as usize]);
                }
                println!("]");
            },
            Err(_) => {
                println!("No msg!");
            }
        }

        min2.hw_if.close();
    }).unwrap();

    app1.join().unwrap();
    app2.join().unwrap();
}
