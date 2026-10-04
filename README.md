![Plugin Icon](assets/icon.png)

# OpenDeck FIFINE Ampligame D6 Plugin

An unofficial [OpenDeck](https://github.com/nekename/OpenDeck) plugin for the FIFINE Ampligame D6.

This is a redone fork of [shugotekitten/opendeck-ampgd6](https://github.com/shugotekitten/opendeck-ampgd6), done SPECIFICALLY for the D6.

## What's different in this fork

- **Fixed icon scaling.** Icons are sent as 100x100px which is the proper fit. Old repo used 105x105px which made all icons seem cut off on the left and top. 🤷‍♂️
- **Much faster profile and page switches.** Images are sent to the d6 as soon as opendeck renders them, keys that don't change are skipped, and large batch changes (like a profile switch) merge into one. 
- **Smoother icons.** Downscales instead of calling for it's neighbor, makes the edges not jagged
- **Better error handling.** Self explanatory 

## Requirements

- OpenDeck 2.5.0 or newer
- FIFINE Ampligame D6 (`3142:0007`)

| Platform | Status |
| --- | --- |
| Linux | Tested |
| Mac | Builds, untested |
| Windows | Builds, untested |

## Installation

1. Download `opendeck-ampgd6.plugin.zip` from the [releases](../../releases) page
2. In OpenDeck: Plugins -> Install from file
3. Linux only: copy [40-opendeck-ampgd6.rules](./40-opendeck-ampgd6.rules) into `/etc/udev/rules.d/` and run `sudo udevadm control --reload-rules`
4. Unplug and replug the device, then restart OpenDeck

If you had the original plugin installed, uninstall it first, then load this one.

## Device specifications

- Layout: 3 rows x 5 columns (15 keys)
- Key screens: 100x100, mounted upside down (has to rotate 180 for it to be proper)
- Protocol version: 1

## Building

You need [podman](https://podman.io) or Docker. Everything else runs in a container.

```sh
./build.sh                    # build linux, mac and windows, zip to build/
./build.sh --install          # same, then install into your local OpenDeck
./build.sh --version 0.3.0    # bump the version first
```

## Credits

- [Andrey Viktorov](https://github.com/4ndv) for the original akp153 plugin this all started from
- Cyrille Babon for adapting it to the Ampligame D6
- [shugotekitten](https://github.com/shugotekitten/opendeck-ampgd6) and everyone who sent fixes there
- Contributors of the [elgato-streamdeck](https://github.com/streamduck-org/elgato-streamdeck) crate and [mirajazz](https://github.com/4ndv/mirajazz)
- [akksmash/omarchy-fifine-deck](https://github.com/akksmash/omarchy-fifine-deck), [ciscosweater/opendeck-ampgd6](https://github.com/ciscosweater/opendeck-ampgd6) and the [Companion](https://github.com/bitfocus/companion-surface-mirabox-stream-dock) D6 work for the protocol notes
