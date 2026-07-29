# `rf64`

Fully Rust-based firmware for the "DJTechTools MIDI Fighter 64" controller.

> [!WARNING]
> This project is still in **very** early development. It is not yet feature-complete, and may not be stable. Use at your own risk.

## 🌠 Motivation

The C firmware is already incredibly well-written and feature-complete. However, there are a couple places where Rust can shine:

- **Safety**: Rust's type system and borrow checker can help prevent many common bugs that are easy to make in C, especially in embedded systems.
- **Concurrency**: Rust's ownership model makes it easier to write concurrent code without fear of data races, which can be a challenge in C.
- **Speed**: Rust's zero-cost abstractions can lead to faster code than C in some cases, especially when it comes to high-level abstractions.

> [!WARNING]
> You will quickly learn that **Rust's speed can (and will) be a double-edged sword**. Sometimes, LLVM may optimize things so well that the underlying hardware will literally not have enough time to keep up with the code.

## 🎛️ Hardware

The MIDI Fighter 64 is a boutique MIDI controller with the following specifications:

- MCU: Atmel AVR ATmega32U4
- LEDs: 128x (2x per button) WS2812B individually-addressable RGB LEDs
- Buttons: 64x SANWA OBSF-30 arcade pushbuttons
- Shift registers: 8x CD4021BM
- USB: Full-speed USB 2.0 Type-B

## 🧰 Building

```bash
cargo build --release --target avr-none
```

## 💉 Flashing

There's actually two ways, depending on whether you want to use the built-in bootloader or not.

### 🧵 Using the ICSP 6-pin header on the PCB

Connect the wires as follows:

| Pin | Function | Color |
| --- | ----- | -------- |
| 1   | MISO | Blue |
| 2   | VCC  | Red |
| 3   | SCK  | White |
| 4   | MOSI | Green |
| 5   | CS/RST | Yellow |
| 6   | GND  | Black |

Then run the following command:

```bash
# This will load the bootloader first...
avrdude -c buspirate -p atmega32u4 -U flash:w:bin/BootloaderDFU_mf64.hex

# ...and then the actual firmware, using -D to disable auto-erase.
avrdude -c buspirate -p atmega32u4 -D -U flash:w:target/rf64.bin
```

Replace `buspirate` with your programmer. During development, the [Bus Pirate](https://web.archive.org/web/20260419123742/http://dangerousprototypes.com/docs/Bus_Pirate) is what I used.

You may also use an Arduino, a Raspberry Pi (make sure to step-down the 5V GPIO to 3.3V or the Raspberry might suffer!), or any other programmer that supports the AVR family. DJTechTools used an `usbtiny` programmer.

### 💿 Using the built-in bootloader

With the built-in DFU bootloader, things get 10x easier.

> [!CAUTION]
> There is a catch: it is *very* easy to soft-brick your controller if you disconnect the USB cable between the `erase` and `flash` commands.
> 
> This is because the bootloader only starts if a specific value is written in RAM. Powering off the device without a valid firmware will reset the device, thus the magic value will get wiped and the bootloader will never start again. You will then need to use the ICSP header to re-flash the device completely.

To flash a new firmware, simply run:

```bash
dfu-programmer atmega32u4 erase
dfu-programmer atmega32u4 flash target/rf64.hex

# Optionally, to reboot the device
dfu-programmer atmega32u4 start
```

## 💛 Acknowledgements

I've solo-built this project, but I would've never been able to do it without:

- [DJTechTools](https://www.djtechtools.com/) and [Shawn Wasabi](https://www.instagram.com/shawnwasabi) for creating the MIDI Fighter 64 and (maybe indirectly) releasing the original firmware source code. Most of the startup code in this project is a literal 1:1 translation, because we cannot improve the bits that are perfect.
- [The Rust Embedded Working Group](https://rust-embedded.org/) and the [avr-rust](https://github.com/avr-rust) people, which allows us to compile Rust code for AVR microcontrollers.
- [Anth](https://github.com/anthonyhfm) for bits of advice on the MF64's hardware, bootloader and firmware.

And lastly, our beloved [Gemini](https://gemini.google.com) for literally carrying this during the black-box debugging of this project.

## 📝 License

This project is licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.

**What this means:**
- ✅ **You can** use this firmware to build other open source things.
- ✅ **You can** modify the firmware to suit your needs.
- 🛑 **You cannot** use this code for network services (although I really doubt you're going to do much with a firmware)
- 🛑 **If you distribute** modified versions of this firmware, you **must** provide the source code as well.

See [LICENSE](LICENSE) for more details.