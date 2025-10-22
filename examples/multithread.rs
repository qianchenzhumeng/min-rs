extern crate min_rs as min;
use env_logger;
use log::LevelFilter;
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex,
};
use std::thread;

struct Uart {
    name: String,
    tx_space_avaliable: u16,
    sender: Sender<u8>,
    receiver: Arc<Mutex<Receiver<u8>>>,
}

impl Uart {
    fn new(
        name: String,
        tx_space_avaliable: u16,
        sender: Sender<u8>,
        receiver: Receiver<u8>,
    ) -> Self {
        Uart {
            name: name,
            tx_space_avaliable: tx_space_avaliable,
            sender: sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    fn open(&self) {
        println!("{}: Open uart.", self.name);
    }

    fn available_for_write(&self) -> u16 {
        self.tx_space_avaliable
    }

    fn tx(&self, byte: u8) {
        print!("0x{:02x} ", byte);
        match self.sender.send(byte) {
            Ok(_) => {}
            Err(e) => {
                println!("{}: {}", self.name, e);
            }
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
    let (tx, rx) = channel();

    let uart = Uart::new(String::from("uart"), 128, tx, rx);
    uart.open();

    let min: Arc<Mutex<min_rs::Context<Uart>>> = Arc::new(Mutex::new(min_rs::Context::new(
        String::from("min"),
        uart,
        0,
        false,
    )));
    let min1 = Arc::clone(&min);
    let min2 = Arc::clone(&min);
    let app1_builder = thread::Builder::new().name("app1".into());
    let app1 = app1_builder
        .spawn(move || loop {
            if let Ok(mut tx) = min1.lock() {
                println!("The data to be sent: {:02X?}", tx_data);

                match tx.send_frame(id, &tx_data, tx_data.len() as u8) {
                    Ok(size) => {
                        println!("{}: {} bytes sent.", tx.hw_if.name, size);
                    }
                    Err(_) => panic!("Opps!"),
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
            break;
        })
        .unwrap();

    let app2_builder = thread::Builder::new().name("app2".into());
    let app2 = app2_builder
        .spawn(move || loop {
            if let Ok(mut min) = min2.lock() {
                let char = if let Ok(rx) = min.hw_if.receiver.lock() {
                    if let Ok(ch) = rx.try_recv() {
                        Some(ch)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(byte) = char {
                    min.poll(&[byte as u8][0..1], 1);
                }
                match min.get_msg() {
                    Ok(msg) => {
                        println!("app2 receive data: {:02X?}", msg.buf);
                        break;
                    }
                    Err(_) => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        })
        .unwrap();

    app1.join().unwrap();
    app2.join().unwrap();
}
