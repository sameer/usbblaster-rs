#![no_std]
#![no_main]

extern crate arduino_mkrvidor4000 as hal;

use hal::clock::GenericClockController;
use hal::delay::Delay;
use hal::entry;
use hal::gpio::IntoFunction;
use hal::pac::{
    gclk::{clkctrl::GEN_A, genctrl::SRC_A},
    interrupt, CorePeripherals, Peripherals, NVIC,
};
use hal::prelude::*;
use hal::usb::usb_device::{bus::UsbBusAllocator, prelude::*};
use hal::usb::UsbBus;
use usbd_serial::{SerialPort, USB_CLASS_CDC};

mod blaster;

// use usb_device::prelude::*;

// mod ft245rom;
static mut USB_ALLOCATOR: Option<UsbBusAllocator<UsbBus>> = None;
static mut USB_BUS: Option<UsbDevice<UsbBus>> = None;
static mut USB_SERIAL: Option<SerialPort<UsbBus>> = None;

#[entry]
fn main() -> ! {
    const ALTERA_BLASTER_USB_VID_PID: UsbVidPid = UsbVidPid(0x09FB, 0x6001);

    let mut peripherals = Peripherals::take().unwrap();
    let mut core = CorePeripherals::take().unwrap();
    let mut clocks = GenericClockController::with_internal_32kosc(
        peripherals.GCLK,
        &mut peripherals.PM,
        &mut peripherals.SYSCTRL,
        &mut peripherals.NVMCTRL,
    );

    // let ft245 = UsbDeviceBuilder::new(bus.allocator());
    let mut pins = hal::Pins::new(peripherals.PORT);

    clocks.configure_gclk_divider_and_source(GEN_A::GCLK6, 1, SRC_A::GCLKGEN1, false);
    let usb_gclk = clocks.get_gclk(GEN_A::GCLK6).unwrap();
    let usb_clock = &clocks.usb(&usb_gclk).unwrap();

    peripherals.USB.device_mut().ctrlb.write(|w| {
        w.spdconf().fs();
        w
    });
    peripherals.USB.device_mut().ctrla.write(|w| {
        w.mode().device();
        w.runstdby().set_bit();
        w.enable().set_bit();
        w
    });

    unsafe {
        USB_ALLOCATOR = UsbBusAllocator::new(UsbBus::new(
            usb_clock,
            &mut peripherals.PM,
            pins.usb_n.into_function(&mut pins.port),
            pins.usb_p.into_function(&mut pins.port),
            peripherals.USB,
        ))
        .into();
        USB_BUS =
            UsbDeviceBuilder::new(&USB_ALLOCATOR.as_ref().unwrap(), ALTERA_BLASTER_USB_VID_PID)
                .manufacturer("Altera")
                .product("USB-Blaster")
                .device_class(USB_CLASS_CDC)
                .device_release(0x0004)
                // .max_packet_size_0(16) // setOutEpSize (?)
                .max_power(450)
                .build()
                .into();
        USB_SERIAL = SerialPort::new(&USB_ALLOCATOR.as_ref().unwrap()).into();
        core.NVIC.set_priority(interrupt::USB, 1);
        NVIC::unmask(interrupt::USB);
        let blaster = blaster::Blaster::new(
            USB_ALLOCATOR.as_ref().unwrap(),
            pins.fpga_tdi,
            pins.fpga_tck.into_push_pull_output(&mut pins.port),
            pins.fpga_tms.into_push_pull_output(&mut pins.port),
            pins.fpga_tdo.into_push_pull_output(&mut pins.port),
        );    
    }
    let mut led = pins.led_builtin.into_open_drain_output(&mut pins.port);
    let mut delay = Delay::new(core.SYST, &mut clocks);

    loop {
        let pending = cortex_m::interrupt::free(|_| unsafe {
            if HIGH {
                led.set_high().unwrap();
            } else {
                led.set_low().unwrap();
            }
        });
    }
}

static mut HIGH: bool = true;

#[interrupt]
fn USB() {
    unsafe {
        USB_BUS.as_mut().map(|usb_dev| {
            USB_SERIAL.as_mut().map(|serial| {
                usb_dev.poll(&mut [serial]);
                let mut buf = [0u8; 64];

                if let Ok(count) = serial.read(&mut buf) {
                    for (i, c) in buf.iter().enumerate() {
                        if i > count {
                            break;
                        }
                        match c.clone() as char {
                            'H' => {
                                HIGH = true;
                            }
                            'L' => {
                                HIGH = false;
                            }
                            _ => {}
                        }
                    }
                };
            });
        });
    };
}
