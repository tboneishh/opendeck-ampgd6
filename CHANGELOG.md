# Changelog

## [0.3.1] - 2026-10-04

### Changes

- Plugin ID is now `com.github.tboneishh.opendeck-ampgd6` instead of the original's `st.lynx` one, so it can go in the OpenAction catalogue. Uninstall the old one first and place the page/sleep buttons again

## [0.3.0] - 2026-10-04

### Features

- Page switcher: next page, previous page and a page counter
- Sleep button: turns the screen off, any key wakes it back up
- Watchdog that USB resets the D6 when its key input freezes (Linux only)
- Mac and Windows builds for basic support, extras are Linux only
- Separate lighter Mac/Windows zip without the Linux only extras

### Bug Fixes

- Fix keys getting stuck looking pressed after a quick double press

## [0.2.2] - 2026-04-19

### 🐛 Bug Fixes

- Fix ghost click on profile switch by forcing the event reader to protocol v3 and reading the actual key state byte instead of emitting a synthetic down+up pair (thanks @theGENreel, #3)
- Sync manifest.json version with Cargo.toml

## [0.2.1] - 2026-02-23

### 🚀 Features

- Add build to release

### 🐛 Bug Fixes

- correct cargo edition to 2024
- remove ref of template plugin name

## [0.2.0] - 2026-01-23

### 🚀 Features

- Adapt size of logo for Ampligame D6 - #1
- Change namespace from 99 to d6 to avoid duplication with the akp153 plugin

## [0.1.0] - 2025-11-28

### 🚀 Features

- First version, fork for akp153 Plugin with modification to work with Fifine Ampligame D6
