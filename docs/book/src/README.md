# Introduction

SynUI is a cross-platform GUI framework for Rust with Material Design 3 theming, retained-mode widgets, and a Vello rendering pipeline.

## Architecture

- **synui-core** — widget trait, event system, focus, gestures
- **synui-layout** — Taffy flexbox layout with RTL support
- **synui-render** — Parley text, iconify icons, Vello scenes
- **synui-winit** — desktop application loop
- **synui-widgets** — MD3 component library

See [DEVELOPMENT.md](../DEVELOPMENT.md) for build instructions.
