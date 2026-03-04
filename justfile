set windows-shell := ["powershell"]
set shell := ["bash", "-cu"]

_default:
  just --list -u

fmt:
  cargo +nightly fmt

check:
  cargo check --package winit-gtk

run:
  cargo run --example window

zip:
  zip -r .archive/winit.zip winit-common/ winit-core/ winit-x11/ winit-wayland/ winit-gtk/

