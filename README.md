

![Plugin Icon](assets/icon.png)

# OpenDeck FIFINE Ampligame D6 Plugin

An unofficial [OpenDeck](https://github.com/nekename/OpenDeck) plugin for the FIFINE Ampligame D6.

> **This is a Linux plugin.** It's built, tested and used on Linux. Mac and Windows builds are included so the D6 gets basic support there too (icons and key presses), but the extra buttons and fixes from this fork probably won't work on them. See [Platforms](#platforms).

This is a redone fork of [shugotekitten/opendeck-ampgd6](https://github.com/shugotekitten/opendeck-ampgd6), done SPECIFICALLY for the D6.

## Video Showcase
https://github.com/user-attachments/assets/6121c276-f6d8-421e-8082-1e2fa6151eb1

## What's different in this fork

- **Fixed icon scaling.** Icons are sent as 100x100px which is the proper fit. Old repo used 105x105px which made all icons seem cut off on the left and top. 🤷‍♂️
- **Much faster profile and page switches.** Images are sent to the d6 as soon as opendeck renders them, keys that don't change are skipped, and large batch changes (like a profile switch) merge into one. 
- **Smoother icons.** Downscales instead of calling for it's neighbor, makes the edges not jagged
- **Better error handling.** Self explanatory 
- **Built in page switcher.** Next, previous and a page counter, see below.
- **Sleep button.** Turns the D6 screen off, any key turns it back on. See below.
- **Fixes itself when the D6 freezes.** Spamming keys can make the D6 stop sending key presses. On Linux the plugin notices and resets it, no replug needed.
- **No more stuck buttons.** Pressing the same key twice really fast used to lose the second release, leaving the key stuck looking pressed. Fixed.

## Page switcher

Find these under the **FIFINE D6** category in OpenDeck:

- **Next Page** / **Previous Page**: go to the next or previous page, wraps around at the ends
- **Page Counter**: shows which page you're on, press it to jump back to page 1

Pages are your profiles named with just a number: `1`, `2`, `3` and so on, in number order. Anything else (like `Default` or a `test` profile) stays out of the rotation. If you don't have any numbered profiles, every profile counts, with `Default` first.

Put the buttons in the same spot on every page. The switch happens on press, and the whole page flips at once when it's ready.

## Sleep button

Also under **FIFINE D6**. Press it and the D6 screen goes fully off (not just dimmed, brightness 0 barely does anything on this thing).

- Press any key to turn it back on. The press still counts, so hitting mute in the dark wakes it and mutes. Pressing the sleep button itself just wakes it
- Everything redraws when it wakes, including anything that changed while it was off (like a mute button flipping)
- Brightness changes while it's off wait until it wakes up

## Requirements

- OpenDeck 2.5.0 or newer
- FIFINE Ampligame D6 (`3142:0007`)

## Platforms

| Platform | Status |
| --- | --- |
| Linux | Tested, everything works |
| Mac | Builds, untested. Basic support only |
| Windows | Builds, untested. Basic support only |

**Basic support** means the D6 should show up, draw icons, and send key presses to OpenDeck like any other deck. The fork's own extras are made for Linux and probably won't work on Mac or Windows:

- **Page switcher**: switching pages goes through OpenDeck over D-Bus on Linux. Mac and Windows fall back to calling OpenDeck directly, which hasn't been tried
- **Sleep button**: should work since it's just a device command, but nobody has tested it
- **Freeze fix**: Linux only. It reads the kernel log and resets the USB port, neither exists on the other two

If you're on Mac or Windows and something works (or doesn't), open an issue.

## Installation

1. Download a zip from the [releases](../../releases) page:
   - `opendeck-ampgd6.plugin.zip`: everything, all platforms. Get this one on Linux
   - `opendeck-ampgd6-macwin.plugin.zip`: lighter Mac/Windows only build with just the sleep button, no page switcher or freeze fix since those don't work there anyway
2. In OpenDeck: Plugins -> Install from file
3. Linux only: copy [40-opendeck-ampgd6.rules](./40-opendeck-ampgd6.rules) into `/etc/udev/rules.d/` and run `sudo udevadm control --reload-rules`
4. Unplug and replug the device, then restart OpenDeck

If you had the original plugin (or this one before 0.3.1) installed, uninstall it first, then load this one. 0.3.1 changed the plugin ID to `com.github.tboneishh.opendeck-ampgd6`, so any page or sleep buttons need to be placed again.

## Device specifications

- Layout: 3 rows x 5 columns (15 keys)
- Key screens: 100x100, mounted upside down (has to rotate 180 for it to be proper)
- Protocol version: 1

## Building

You need [podman](https://podman.io) or Docker. Everything else runs in a container.

```sh
./build.sh                    # build linux, mac and windows + the mac/win lite zip, all in build/
./build.sh --install          # same, then install into your local OpenDeck
./build.sh --version 0.3.0    # bump the version first
```

## Credits

- [Andrey Viktorov](https://github.com/4ndv) for the original akp153 plugin this all started from
- Cyrille Babon for adapting it to the Ampligame D6
- [shugotekitten](https://github.com/shugotekitten/opendeck-ampgd6) and everyone who sent fixes there
- Contributors of the [elgato-streamdeck](https://github.com/streamduck-org/elgato-streamdeck) crate and [mirajazz](https://github.com/4ndv/mirajazz)
- [akksmash/omarchy-fifine-deck](https://github.com/akksmash/omarchy-fifine-deck), [ciscosweater/opendeck-ampgd6](https://github.com/ciscosweater/opendeck-ampgd6) and the [Companion](https://github.com/bitfocus/companion-surface-mirabox-stream-dock) D6 work for the protocol notes
