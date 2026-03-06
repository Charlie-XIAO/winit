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

run $WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS="1":
  cargo run --example webkitgtk_multiwindow --features glib

perf $WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS="1" RUSTFLAGS="-C debuginfo=1 -C force-frame-pointers=yes":
  cargo build --example webkitgtk_multiwindow --features glib
  perf record -F 99 -g --call-graph fp --all-user -- ./target/debug/examples/webkitgtk_multiwindow
  perf report

zip prefix:
  zip -r .archive/winit-{{prefix}}.zip \
    winit/ \
    winit-common/ \
    winit-core/ \
    winit-x11/ \
    winit-wayland/
