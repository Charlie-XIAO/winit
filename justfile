set windows-shell := ["powershell"]
set shell := ["bash", "-cu"]

_default:
  just --list -u

fmt:
  cargo +nightly fmt
  taplo fmt

check:
  cargo check \
    --package winit-x11 \
    --package winit-wayland \
    --features winit-x11/glib \
    --features winit-wayland/glib

run:
  cargo run --example window

zip:
  zip -r .archive/winit.zip \
    winit/ \
    winit-common/ \
    winit-core/ \
    winit-x11/ \
    winit-wayland/
