#![no_std]
#![no_main]

mod board;
mod dap_usb;
mod protocol;
pub mod swd;
mod wiring;

use core::fmt::Write as _;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
};
use embassy_stm32::can::Frame;
use embassy_time::{with_timeout, Duration};
use heapless::String;

use board::Usb;
use embassy_executor::Spawner;
use embassy_futures::{join::join4, select::select};
use embassy_stm32::can::Can;
use embassy_stm32::usart::BufferedUart;
use embassy_usb::{
    Builder,
    class::cdc_acm::{CdcAcmClass, State},
};
use embedded_io_async::{Read, Write};
use panic_halt as _;
use static_cell::StaticCell;
use swd::Swd;

const PACKET: usize = 64;
static CAN_TX_QUEUE: Channel<CriticalSectionRawMutex, Frame, 8> =
    Channel::new();

static USB_TX_QUEUE: Channel<CriticalSectionRawMutex, String<63>, 16> =
    Channel::new();

fn queue_usb_message(message: &str) {
    if let Ok(text) = String::<63>::try_from(message) {
        let _ = USB_TX_QUEUE.try_send(text);
    }
}
// Track linker-script changes so Cargo relinks the firmware.
const _: &[u8] = include_bytes!("../memory.x");

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let (driver, ports, swd) = wiring::configure(board::init());
    // Classroom-only test VID/PID, not allocated product identifiers.
    let mut config = embassy_usb::Config::new(0xC0DE, 0xCAFC);
    config.manufacturer = Some("HerKules");
    config.product = Some("DM-MC02 CMSIS-DAP Demo");
    config.max_packet_size_0 = PACKET as u8;
    static DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS: StaticCell<[u8; 128]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    static CTL_STATE: StaticCell<State> = StaticCell::new();
    static UART_STATE: StaticCell<State> = StaticCell::new();
    let mut builder = Builder::new(
        driver,
        config,
        DESC.init([0; 256]),
        BOS.init([0; 128]),
        MSOS.init([0; 256]),
        CONTROL.init([0; 64]),
    );
    builder.msos_descriptor(0x0603_0000, 0x20);
    let dap = dap_usb::DapUsb::new(&mut builder);
    let control = CdcAcmClass::new(&mut builder, CTL_STATE.init(State::new()), PACKET as u16);
    let serial = CdcAcmClass::new(&mut builder, UART_STATE.init(State::new()), PACKET as u16);
    let mut device = builder.build();
    join4(
        device.run(),
        control_port(control, ports.can, swd),
        uart_port(serial, ports.uart),
        dap.run(),
    )
    .await;
}

async fn control_port(
    usb: CdcAcmClass<'static, Usb>,
    can: Can<'static>,
    mut swd: Option<Swd>,
) {
    let (mut usb_tx, mut usb_rx) = usb.split();
    let (mut can_tx, mut can_rx, _props) = can.split();
    
    // Task 1: USB -> CAN
    let usb_reader = async {
        let mut packet = [0u8; PACKET];
        loop {
            usb_rx.wait_connection().await;
            let mut line = protocol::Line::new();

            while let Ok(n) = usb_rx.read_packet(&mut packet).await {
                for &byte in &packet[..n] {
                    let Some(command) = line.push(byte) else {
                        continue;
                    };

                    if command == "PING" {
                        queue_usb_message("OK DM-MC02\n");
                    }else if command == "SWD ID" {
                        if let Some(swd) = swd.as_mut() {
                            if let Some(id) = swd.read_dpidr() {
                                let mut reply = String::<63>::new();
                                let _ = writeln!(&mut reply, "OK DPIDR {id:08X}");
                                let _ = USB_TX_QUEUE.try_send(reply);
                            } else {
                                queue_usb_message("ERR SWD\n");
                            }
                        } else {
                            queue_usb_message("ERR SWD_NOT_CONFIGURED\n");
                        }
                    } else if command == "CAN RX" {
                        queue_usb_message("ERR USE STREAM MODE\n");
                    }else if let Some(frame) = protocol::parse_bridge_tx(command) {
                        if CAN_TX_QUEUE.try_send(frame).is_ok() {
                            queue_usb_message("OK QUEUED\n");
                        } else {
                            queue_usb_message("ERR CAN QUEUE FULL\n");
                        }
                    } else {
                        queue_usb_message("ERR SYNTAX\n");
                    }
                }
            }
            CAN_TX_QUEUE.clear();
        }
    };

    // Task 2: Send CAN frames
    let can_writer = async {
        loop {
            let frame = CAN_TX_QUEUE.receive().await;

            match with_timeout(Duration::from_millis(100), can_tx.write(&frame)).await {
                Ok(None) => queue_usb_message("OK CAN TX\n"),
                _ => queue_usb_message("ERR CAN TX\n"),
            }
        }
    };

    // Task 3: CAN -> USB
    let can_reader = async {
        loop {
            match can_rx.read().await {
                Ok(envelope) => {
                    if let Some(message) = protocol::format_bridge_rx(&envelope.frame) {
                        let _ = USB_TX_QUEUE.try_send(message);
                    }
                }
                Err(_) => {
                    queue_usb_message("ERR CAN RX\n");
                    embassy_time::Timer::after_millis(10).await;
                }
            }
        }
    };

    // Task 4: USB output
    let usb_writer = async {
        loop {
            usb_tx.wait_connection().await;

            loop {
                let message = USB_TX_QUEUE.receive().await;
                let result = with_timeout(Duration::from_millis(200), usb_tx.write_packet(message.as_bytes())).await;
                match result {
                    Ok(Ok(())) => {}
                    _ => { USB_TX_QUEUE.clear(); break; }
                }
            }
        }
    };

    join4(usb_reader, can_writer, can_reader, usb_writer).await;
}

async fn uart_port(usb: CdcAcmClass<'static, Usb>, uart: Option<BufferedUart<'static>>) {
    let Some(uart) = uart else {
        return;
    };
    let (mut serial_out, mut serial_in) = uart.split();
    let (mut usb_out, mut usb_in) = usb.split();
    loop {
        usb_in.wait_connection().await;
        let from_host = async {
            let mut buf = [0; PACKET];
            while let Ok(n) = usb_in.read_packet(&mut buf).await {
                if serial_out.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        };
        let to_host = async {
            // A short packet needs no zero-length terminator.
            let mut buf = [0; PACKET - 1];
            while let Ok(n) = serial_in.read(&mut buf).await {
                if n != 0 && usb_out.write_packet(&buf[..n]).await.is_err() {
                    break;
                }
            }
        };
        select(from_host, to_host).await;
    }
}
