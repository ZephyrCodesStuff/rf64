use usb_device::bus::UsbBusAllocator;
use usb_device::device::{UsbDevice, UsbDeviceBuilder, UsbVidPid};
use usbd_midi::midi_device::MidiClass;

pub type UsbBus = atmega_usbd::UsbBus<()>;

/// Enable the 48 MHz USB PLL from the 16 MHz crystal on ATmega32U4.
/// Matches ATmega32U4 datasheet & LUFA USB_OPT_AUTO_PLL logic.
pub fn init_usb_pll() {
    unsafe {
        // PLLCSR register is at SRAM 0x49 (I/O 0x29)
        // Bit 4 = PINDIV (1 = 16 MHz crystal divided by 2 -> 8 MHz PLL input)
        // Bit 1 = PLLE (PLL Enable)
        // Bit 0 = PLOCK (PLL Lock Status)

        // Set PINDIV for 16MHz crystal and enable PLL
        core::ptr::write_volatile(0x49 as *mut u8, (1 << 4) | (1 << 1));

        // Wait until PLL achieves lock (PLOCK bit 0 set)
        while (core::ptr::read_volatile(0x49 as *const u8) & (1 << 0)) == 0 {}
    }
}

/// DJTT Midi Fighter 64 USB Identifiers
pub const VID: u16 = 0x2580; // DJ TechTools
pub const PID: u16 = 0x0008; // Midi Fighter 64

/// Create a UsbBusAllocator using ATmega32U4 USB_DEVICE peripheral.
pub fn create_usb_bus(usb: atmega_hal::pac::USB_DEVICE) -> UsbBusAllocator<UsbBus> {
    atmega_usbd::UsbBus::new(usb)
}

pub struct UsbMidiStack<'a> {
    pub usb_dev: UsbDevice<'a, UsbBus>,
    pub midi: MidiClass<'a, UsbBus>,
}

impl<'a> UsbMidiStack<'a> {
    pub fn new(bus: &'a UsbBusAllocator<UsbBus>) -> Self {
        let mut midi = MidiClass::new(bus);

        let mut usb_dev = UsbDeviceBuilder::new(bus, UsbVidPid(VID, PID))
            .manufacturer("https://github.com/ZephyrCodesStuff/rf64")
            .product("Rusty Fighter 64")
            .serial_number(r#"¯\_(ツ)_/¯"#)
            .device_class(0x00)
            .device_sub_class(0x00)
            .device_protocol(0x00)
            .max_power(480)
            .build();

        // We must poll once to trigger bus.enable() so that USBE=1, otherwise UDCON write hangs!
        usb_dev.poll(&mut [&mut midi]);

        // Force a robust 100ms USB detach to ensure macOS recognizes the reset from bootloader
        unsafe {
            core::ptr::write_volatile(0xE0 as *mut u8, 1);
        }
        crate::delay::delay_ms(100);
        unsafe {
            core::ptr::write_volatile(0xE0 as *mut u8, 0);
        }

        UsbMidiStack { usb_dev, midi }
    }

    pub fn poll(&mut self) -> bool {
        self.usb_dev.poll(&mut [&mut self.midi])
    }
}
