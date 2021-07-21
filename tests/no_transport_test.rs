#[cfg(test)]
mod tests {
    struct App {
        name: String,
    }

    impl App {
        fn print_msg(&self, _buffer: &[u8], _len: u8) {}
    }

    impl min::Name for App {
        fn name(&self) -> String {
            self.name.clone()
        }
    }

    struct Uart {
        tx_space_avaliable: u16,
    }

    impl Uart {
        fn open(&self) {}
        fn close(&self) {}
        fn tx(&self, _byte: u8) {}
        fn available_for_write(&self) -> u16 {
            self.tx_space_avaliable
        }
    }

    fn tx_start(_: &Uart) {}
    fn tx_finished(_: &Uart) {}
    fn tx_space(uart: &Uart) -> u16 {
        uart.available_for_write()
    }
    fn tx_byte(uart: &Uart, _port: u8, byte: u8) {
        uart.tx(byte);
    }
    fn application_handler(app: &App, _min_id: u8, buffer: &[u8], len: u8, _port: u8) {
        app.print_msg(buffer, len);
    }

    #[test]
    fn send() {
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
        };
        let id: u8 = 0;
        let payload: [u8; 255] = [0; 255];
        let len: u8 = 8;
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            false,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        let sent = match min.send_frame(id, &payload, len) {
            Ok(size) => size,
            Err(_) => panic!("Opps!"),
        };

        uart.close();

        assert_eq!(len, sent);
    }

    #[test]
    fn send_no_enough_buffer() {
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
        };
        let overfllow: u16 = 1;
        let id: u8 = 0;
        let payload: [u8; 255] = [0; 255];
        let len: u8 = uart.available_for_write() as u8 + overfllow as u8;
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            false,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        let oversized = match min.send_frame(id, &payload, len) {
            Ok(_) => 0,
            Err(min::Error::NoEnoughTxSpace(size)) => size,
        };

        uart.close();

        assert_eq!(overfllow, oversized);
    }

    #[test]
    fn receive() {
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
        };
        let payload: [u8; 19] = [
            0xaa, 0xaa, 0xaa,   // SOF
            0x00,   // ID/control
            0x08,   // Length
            0xaa, 0xaa, 0x55, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x00,   // Data(including a stuff-byte 0x55)
            0x38, 0x83, 0x8f, 0x82, // CRC checksum
            0x55,   // EOF
        ];
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            false,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        min.poll(&payload, payload.len() as u32);

        uart.close();

        assert_eq!(0x38838f82, min.get_rx_frame_checksum());
    }

    #[test]
    fn restart_receiving() {
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
        };
        let payload: [u8; 24] = [
            0xaa, 0xaa, 0xaa,   // SOF
            0x00,   // ID/control
            0x08,   // Length
            0xaa, 0xaa, 0xaa,   // SOF
            0x00,   // ID/control
            0x08,   // Length
            0xaa, 0xaa, 0x55, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x00,   // Data(including a stuff-byte 0x55)
            0x38, 0x83, 0x8f, 0x82, // CRC checksum
            0x55,   // EOF
        ];
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            false,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        min.poll(&payload, payload.len() as u32);

        uart.close();

        assert_eq!(0x38838f82, min.get_rx_frame_checksum());
        assert_eq!(8, min.get_rx_frame_len());
    }

    #[test]
    fn receive_frame_error_length() {
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
        };
        let payload: [u8; 19] = [
            0xaa, 0xaa, 0xaa,   // SOF
            0x00,   // ID/control
            0x02,   // Length(correct value is 8)
            0xaa, 0xaa, 0x55, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x00,   // Data(including a stuff-byte 0x55)
            0x38, 0x83, 0x8f, 0x82, // CRC checksum
            0x55,   // EOF
        ];
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            false,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        min.poll(&payload, payload.len() as u32);

        uart.close();

        // 0x38838f82 is the checksum in payload
        assert_ne!(0x38838f82, min.get_rx_frame_checksum());
        // 0xaa000000 is the checksum according to "Length" field(0x02)
        assert_eq!(0xaa000000, min.get_rx_frame_checksum());
        // 0x402b15e7 is the checksum of [0x00, 0x02, 0xaa, 0xaa]
        assert_eq!(0x402b15e7, min.get_rx_checksum());
    }

    #[test]
    fn receive_frame_error_checksum() {
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
        };
        let payload: [u8; 19] = [
            0xaa, 0xaa, 0xaa,   // SOF
            0x00,   // ID/control
            0x08,   // Length
            0xaa, 0xaa, 0x55, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x00,   // Data(including a stuff-byte 0x55)
            0x00, 0x83, 0x8f, 0x82, // CRC checksum(correct value is [0x38, 0x83, 0x8f, 0x82])
            0x55,   // EOF
        ];
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            false,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        min.poll(&payload, payload.len() as u32);

        uart.close();

        // 0x00838f82 is the checksum in payload
        assert_eq!(0x00838f82, min.get_rx_frame_checksum());
        // 0x38838f82 is the correct checksum
        assert_eq!(0x38838f82, min.get_rx_checksum());
    }
}
