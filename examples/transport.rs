extern crate min_rs as min;
use std::thread;
use std::sync::mpsc::{Sender, Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use log::{LevelFilter, info, debug, trace};
use env_logger;

struct App {
    name: String,
}

impl min::Name for App {
    fn name(&self) -> String {
        self.name.clone()
    }
}

impl App {
    fn new(name: String) -> Self {
        App{
            name: name,
        }
    }
    fn print_msg(&self, buffer: &[u8], len: u8) {
        let mut output = String::from("");
        output.push_str(format!("receive data: [ ").as_str());
        for i in 0..len {
            output.push_str(format!("0x{:02x} ", buffer[i as usize]).as_str());
        }
        output.push_str(format!("]").as_str());
        info!(target: self.name.as_str(), "{}", output);
    }
}

struct Uart {
    name: String,
    tx_space_avaliable: u16,
    sender: Sender<u8>,
    receiver: Receiver<u8>,
    output: Arc<Mutex<String>>,
}

impl Uart {
    fn new(name: String, tx_space_avaliable: u16, sender: Sender<u8>, receiver: Receiver<u8>) -> Self {
        Uart{
            name: name,
            tx_space_avaliable: tx_space_avaliable,
            sender: sender,
            receiver: receiver,
            output: Arc::new(Mutex::new(String::from(""))),
        }
    }

    fn open(&self) {
        debug!(target: self.name.as_str(), "{}: Open uart.", self.name);
    }

    //fn close(&self) {
    //    debug!(target: self.name.as_str(), "{}: Close uart.", self.name);
    //}

    fn available_for_write(&self) -> u16 {
        self.tx_space_avaliable
    }

    fn tx(&self, byte: u8) {
        let mut output = self.output.lock().unwrap();
        output.push_str(format!("0x{:02x} ", byte).as_str());
        match self.sender.send(byte) {
            Ok(_) => {},
            Err(e) => {
                debug!(target: self.name.as_str(), "{}", e);
            },
        }
    }
}

fn tx_start(uart: &Uart) {
    let mut output = uart.output.lock().unwrap();
    output.clear();
    output.push_str(format!("send frame: [ ").as_str());
}

fn tx_finished(uart: &Uart) {
    let mut output = uart.output.lock().unwrap();
    output.push_str(format!("]").as_str());
    trace!(target: uart.name.as_str(), "{}", output);
}
fn tx_space(uart: &Uart) -> u16 {
    uart.available_for_write()
}

fn tx_byte(uart: &Uart, _port: u8, byte: u8) {
    uart.tx(byte);
}

fn application_handler(app: &App, _min_id: u8, buffer: &[u8], len: u8, _port: u8) {
    app.print_msg(buffer, len);
}

fn rx_byte(min: &mut min::Context<Uart, App>, buf: &[u8], buf_len: u32) {
    min.poll(buf, buf_len);
}

fn main() {
    let id: u8 = 0;
    let tx_data1: [u8; 8] = [0xaa, 0xaa, 0xaa, 0, 0, 0, 0, 1];
    let tx_data2: [u8; 8] = [0xbb, 0xbb, 0xbb, 0, 0, 0, 0, 1];
    let app1 = App::new(String::from("app1"));
    let app2 = App::new(String::from("app2"));
    let (tx1, rx2) = channel();
    let (tx2, rx1) = channel();

    log::set_max_level(LevelFilter::Debug);
    env_logger::init();

    info!("hello");

    let app1_builder = thread::Builder::new().name("app1".into());
    let app1 = app1_builder.spawn(move || {
        let uart1 = Uart::new(String::from("uart1"), 128, tx1, rx1);
        let mut min1 = min::Context::new(
            &uart1,
            &app1,
            0,
            true,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );
        min1.hw_if.open();

        thread::sleep(Duration::from_secs(5));
        min1.queue_frame(id, &tx_data1, tx_data1.len() as u8).unwrap();
        min1.queue_frame(id, &tx_data1, tx_data1.len() as u8).unwrap();
        loop {
            for byte in min1.hw_if.receiver.try_iter() {
                rx_byte(&mut min1, &[byte as u8][0..1], 1);
            }
            min1.poll(&[0][0..0], 0);
            thread::sleep(Duration::from_millis(100));
        }

        //min1.hw_if.close();
    }).unwrap();
    
    let app2_builder = thread::Builder::new().name("app2".into());
    let app2 = app2_builder.spawn(move || {
        let uart2 = Uart::new(String::from("uart2"), 128, tx2, rx2);
        let mut min2 = min::Context::new(
            &uart2,
            &app2,
            0,
            true,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );
        min2.hw_if.open();

        min2.queue_frame(id, &tx_data2, tx_data2.len() as u8).unwrap();
        min2.queue_frame(id, &tx_data2, tx_data2.len() as u8).unwrap();
        loop {
            for byte in min2.hw_if.receiver.try_iter() {
                rx_byte(&mut min2, &[byte as u8][0..1], 1);
            }
            min2.poll(&[0][0..0], 0);
            thread::sleep(Duration::from_millis(100));
        }

        //min2.hw_if.close();
    }).unwrap();

    app1.join().unwrap();
    app2.join().unwrap();
}
