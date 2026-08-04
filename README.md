<div align="center">

  <h1>🦀 <code>rf64</code> 🎹</h1>

  <p>
    <strong>A completely Rust-based, bare-metal firmware for the MIDI Fighter 64.</strong>
  </p>

  <p>
    <a href="https://github.com/ZephyrCodesStuff/rf64/actions"><img src="https://img.shields.io/github/actions/workflow/status/ZephyrCodesStuff/rf64/lint.yml?branch=main&style=flat-square" alt="Build Status"></a>
    <a href="#license"><img src="https://img.shields.io/badge/license-AGPLv3-blue?style=flat-square" alt="License"></a>
  </p>

</div>

RF64 is a custom-made bare-metal firmware for the [MIDI Fighter 64](https://www.midifighter.com/#64) music controller. It aims to improve stability, hardware safety, performance and overall user-experience by adding commonly used features (such as full Apollo Studio support, and eventually custom color palettes).

To get started, you can grab the latest `.hex` file [here](https://github.com/ZephyrCodesStuff/rf64/releases/latest) and flash it with the official [DJTT MIDI Fighter Utility](https://store.djtechtools.com/pages/midi-fighter-utility) app!

---

## 🌟 Authors

- [@zeph](https://github.com/ZephyrCodesStuff) (that's me!)

## 🌠 Features

- **Performance**: Clever tricks such as parallel LED strand driving, optimized loop cycles, and zero-cost abstractions for minimal latency during the most intense lightshows.
- **Boot Animation**: An interactive Snake game animation that appears at boot, or after 256 seconds of idling. It stops immediately as soon as a MIDI message is received or a button is pressed, ensuring it never gets in your way.
- **Keyboard Emulation**: Ultra-low latency HID boot keyboard mode (ideal for rhythm games like _Osu!_, _Geometry Dash_ or just general use).
- **Full [Apollo Studio](https://github.com/mat1jaczyyy/apollo-studio) Support**: Complete FastRGB and SysEx MIDI protocol support for Apollo Studio lightshows.
- **Overcurrent Protection**: Built-in dynamic power scaling logic to ensure the controller never draws more than 480 mA. _Not even the official firmware has this!_ It prevents mid-show USB port brownouts, device restarts, or thermal stress on components.
- **Full Bootloader Compatibility**: Easily flash back to Official Firmware (OFW) at any time using the official MIDI Fighter Utility (MFU) app.

## 🎛️ Hardware

The MIDI Fighter 64 is a boutique MIDI controller with the following specifications:

- **MCU**: Atmel AVR ATmega32U4
- **LEDs**: 128x (2x per button) WS2812B individually-addressable RGB LEDs
- **Buttons**: 64x SANWA OBSF-30 arcade pushbuttons
- **Shift registers**: 8x CD4021BM
- **USB**: Full-speed USB 2.0 Type-B

## 🧰 Building

```bash
cargo build --release
```

### 🚩 Feature Flags & Flash Budget

This firmware uses Cargo feature flags to manage flash usage on the ATmega32U4:

| Feature Flag | Description                                    | Default      |
| ------------ | ---------------------------------------------- | ------------ |
| `boot-anim`  | Snake game boot animation & idle screen saver  | **Enabled**  |
| `apollo`     | Apollo Studio FastRGB & SysEx protocol support | **Enabled**  |
| `keyboard`   | USB HID Boot Keyboard emulation mode           | **Disabled** |

#### Why `keyboard` is disabled by default:

The ATmega32U4 has **28 KB** of usable flash memory reserved for application code (4 KB is reserved for the DFU bootloader).

When compiled with `opt-level = 3` (maximum speed), enabling all three features (`boot-anim`, `apollo`, and `keyboard`) pushes the total flash consumption from ~27 KB up to **~32 KB**, which exceeds the 28 KB limit, meaning we would have to sacrifice the bootloader which is _not a good idea_.

While changing `opt-level = "z"` (size optimization) allows all features to fit into flash, it noticeably degrades high-speed LED lightshow rendering performance. `opt-level = 1` results in oversized binaries, and `opt-level = 2` produces results similar to level 3.

Therefore, you have two build choices depending on your priority:

- 🚀 **Performance (Recommended)**: Keep `opt-level = 3` and use default features (`boot-anim` + `apollo`, no `keyboard`).
- 🎹 **Features**: Set `opt-level = "z"` in `Cargo.toml` and build with `--features keyboard` if keyboard emulation is required:

```bash
cargo build --release --features keyboard
```

## 💉 Flashing

> [!NOTE]
> **Before you flash this, please be aware:**
> 
> This project is *community-made firmware*. While it will not brick your MF64, **don't try flashing it if you're not sure of what you're doing**. As this is open-source software licensed AGPL-3.0, friendly reminder that **you are taking full responsibility**. _DJTT's official firmware works great, too!_
>
> Additionally, this firmware does not have the capability to permanently destroy your device; in the worst possible case that the flashing process goes wrong and you're left with just the bootloader, a technician can always recover the MCU via **ICSP (In-Circuit Serial Programming)**.
>
> **This fragility is a fundamental limitation of DJTT's LUFA bootloader**, and would require a custom bootloader (plus ICSP to rewrite it), in order to fix.

There's actually two ways to flash the firmware, depending on whether you want to use the built-in bootloader or not.

### 🧵 Using the ICSP 6-pin header on the PCB

Connect the wires as follows (*you can pick your favorite colors, these are just what I like*):

| Pin | Function | Color  |
| --- | -------- | ------ |
| 1   | MISO     | Blue   |
| 2   | VCC      | Red    |
| 3   | SCK      | White  |
| 4   | MOSI     | Green  |
| 5   | CS/RST   | Yellow |
| 6   | GND      | Black  |

Then run the following command:

```bash
# This will load the bootloader first...
avrdude -c buspirate -p atmega32u4 -U flash:w:bin/BootloaderDFU_mf64.hex

# ...and then the actual firmware, using -D to disable auto-erase.
avrdude -c buspirate -p atmega32u4 -D -U flash:w:target/rf64.bin
```

> [!NOTE]
> Replace `buspirate` with your programmer.
>
> During development, the [Bus Pirate](https://web.archive.org/web/20260419123742/http://dangerousprototypes.com/docs/Bus_Pirate) is what _I_ used.

You may also use an Arduino, a Raspberry Pi (make sure to step-down the 5V GPIO to 3.3V or the Raspberry might suffer!), or any other programmer that supports the AVR family. DJTechTools used an `usbtiny` programmer.

### 💿 Using the built-in bootloader

With the built-in DFU bootloader, things get 10x easier.

> [!CAUTION]
> There is a catch: it is _very_ easy to soft-brick your controller if you disconnect the USB cable between the `erase` and `flash` commands.
>
> This is because the bootloader only starts if a specific value is written in RAM. Powering off the device without a valid firmware will reset the device, thus the magic value will get wiped and the bootloader will never start again. You will then need to use the ICSP header to re-flash the device completely.
>
> **Always finish flashing before removing the USB cable or using `start` to reboot the device.**

To flash a new firmware, you'll first have to start the MF64 in **DFU bootloader mode**.

1. Unplug it, if you haven't already
2. Press the button on the very bottom-left
3. While keeping the button pressed down, plug the MF64's USB back in and wait 2 seconds

If everything went correctly, in your devices you should now see something like "Midi Fighter DFU Bootloader".

### 🎹 Using the official "MIDI Fighter Utility" app

Open the app, go to the top and select **Tools -> Load custom firmware -> for a 64** and pick the `.hex` file you downloaded from [here](https://github.com/ZephyrCodesStuff/rf64/releases/latest).

### 💻 Using the [dfu-programmer](https://dfu-programmer.github.io) terminal utility

Download a prebuilt binary for the [dfu-programmer](https://dfu-programmer.github.io) utility, and then run:

```bash
# ! DO NOT UNPLUG THE DEVICE IN-BETWEEN THESE !
# Replace `target/rf64.hex` with the path to your downloaded `.hex` file
dfu-programmer atmega32u4 erase
dfu-programmer atmega32u4 flash target/rf64.hex

# ONLY AFTER FLASHING, optionally, to reboot the device:
dfu-programmer atmega32u4 start
```

### 🔁 Rolling back to OFW

To rollback using the MIDI Fighter Utility, you **must** manually boot into DFU bootloader mode (hold bottom-left button on startup). The MFU app will intentionally not detect the device in normal operating mode to prevent it from issuing unsupported OFW commands (such as saving custom color profiles).

You will then see an **orange checkerboard pattern** on your device. This indicates you successfully entered bootloader mode. Simply open the MFU app now, select **64** from the list of devices (this is REALLY important) and wait patiently.

> [!CAUTION]
> **Do not disconnect the device until you see the DJTT boot animation.**
>
> Follow standard safety guidelines when flashing low-level firmware (don't use a janky cable, don't use a USB hub, close all apps that may interfere...)

## 💛 Acknowledgements

I've solo-built this project, but I would've never been able to do it without:

- [DJTechTools](https://www.djtechtools.com/) and [Shawn Wasabi](https://www.instagram.com/shawnwasabi) for creating the MIDI Fighter 64 and (maybe indirectly) releasing the original firmware source code. Most of the startup code in this project is a literal 1:1 translation, because we cannot improve the bits that are perfect.
- [The Rust Embedded Working Group](https://rust-embedded.org/) and the [avr-rust](https://github.com/avr-rust) people, which allows us to compile Rust code for AVR microcontrollers.
- [Anth](https://github.com/anthonyhfm) for bits of advice on the MF64's hardware, bootloader and firmware.

And lastly, our beloved [Gemini](https://gemini.google.com) and [Claude](https://claude.ai) for literally carrying this during the black-box debugging of this project.

## 📝 License

This project is licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.

**What this means:**

- ✅ **You can** use this firmware to build other open source things.
- ✅ **You can** modify the firmware to suit your needs.
- 🛑 **You cannot** use this code for network services (although I really doubt you're going to do much with a firmware)
- 🛑 **If you distribute** modified versions of this firmware, you **must** provide the source code as well.

See [LICENSE](LICENSE) for more details.
